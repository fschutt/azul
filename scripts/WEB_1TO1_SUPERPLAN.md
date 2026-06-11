# Web 1:1 SUPER-PLAN — make the web backend run azul apps like the desktop

**Goal:** everything in `AzCallbackInfo_*` + the full input/output surface usable on the web.
**Method (user directives 2026-06-11):** iterate in increasing complexity; ALWAYS relift + verify
via CDP after each slice; FIX lift bugs found along the way (in `dll/src/web/` transpiler or the
remill fork, never azul-source hacks); keep relifts fast via smart caching. IGNORE layout-number
mismatches (azul-calc vs browser) — user-deferred. Track progress in THIS file (update before
ending every session). Cron `…` fires :13/:43 — when all slices are ✅ or hard-blocked: update
this file, CronList → CronDelete.

**Read first each session:** this file → `scripts/SESSION_PROMPT_web_1to1.md` (hard rules, CDP
loop, build commands) → `scripts/WEB_BACKEND_1TO1_PLAN.md` (architecture reference).

---
## 🔄 POST-REBASE STATE (2026-06-11 ~13:45) — rebased onto mobile-ios-android 82171735d; THREE new engine fixes landed; class-B deep fix IN PROGRESS
User decided: rebase → backup → **class-B deep fix** → ua_css (S7). Rebase done (27 commits
replayed, zero contribution drift, dll/src/web byte-identical; Cargo.lock fixup 84bac13bb;
codegen regenerated — upstream changed struct sizes, C bins rebuilt against fresh azul.h).
Backup force-pushed (e575cebb5 → 808c0320a; pre-rebase tip preserved in that remote commit).

**Post-rebase trap-peeling (each layer a REAL lift gap exposed by upstream's 208 commits
shifting code into latent holes; all fixed + committed 7c6f50546 + diag 808c0320a):**
1. **macOS TLV (thread-locals)**: lifted `adrp x0,<desc>; ldr x8,[x0]; blr x8` fell into the
   dispatcher's unknown-drop → X0 stayed = descriptor addr → std read descriptor bytes as the
   LocalKey state byte → `panic_access_error` (first hit: RandomState KEYS in `HashSet::new`
   inside get_loaded_font_ids). FIX: TlvRegion geometry (symbol_table) + mirror-seed at the
   chokepoint + thunk→AZ_TLV_MAGIC_PC (0xA271C0DE) rewrite + dispatcher case computing
   `X0 = tls_base_synth + desc.offset`. TLS = statics (single-threaded wasm).
2. **Heap/synth-band collision**: bump base 96MiB but the rebased dylib's synth band ends
   ~101MiB → allocations STOMPED mirrored data (TLV descriptors read as heap garbage). FIX:
   bump base → 160MiB (memory is 512MiB). ⚠ This is the standing "memory-map audit" class —
   any future dylib growth past 160MiB re-collides (warn: check `TLV: seeded` log's
   tls_base_synth on relifts).
3. **probe.rs under web_lift**: with TLS *working*, probe spans flipped from
   harmlessly-failing `try_with` to actually running → `Instant::now`/`task_info` mach
   syscalls (unliftable). FIX: probe imp → no-op module under web_lift; sample_peak_rss gated.
4. Loader `solveLayout` try/catch (bootstrap survives traps; __azProbe installs → peek-based
   debugging works on a trapped page — this unlocked the whole hunt).

**Verified progress**: thread-locals initialize; get_loaded_font_ids runs 3× over a 3-font map
(iteration fine!); font chains resolve (chains.len=1); phases pass 0x82. **REMAINING TRAP**
(hello-world still counter-5): text3 `reorder_logical_items` → Vec reserve → `finish_grow`
traps; bump_ptr blows past memory (0x28a529e0 ≈ 681MB from 160MiB base) with last_alloc diag
low-words 0 ⇒ **garbage HIGH WORD in allocation size — the class-B multi-word family**, now in
the text-shaping path (upstream's kerning/coverage work exercises it differently). This is the
NEW class-B witness (alongside getHitNode 16B + build_compact 176B sret).

**Diag markers live** (all [az-diag REVERT]-tagged): get_loaded_font_ids 0x60780..C;
prev_font_hashes header 0x60794..C; merge bracket 0x607A0..A8; phase 0x60704; chains 0x60770;
reorder len 0x607B0..C4; create_logical push 0x607C8..DC; **AzString raw words 0x607E0..F0**.
Peek tooling: /tmp/cdp_peek_multi.js (any addr list), /tmp/cdp_first_trap.js (pause-on-exc
stack), /tmp/cdp_solve_stack.js (re-invoke solve). Bump diag: size@0x40050(i64) count@0x40058
ptr@0x40060 bump@0x40020.

## 🎯 CLASS-B BREAKTHROUGH (2026-06-11 ~14:40): root-caused + FIXED mechanism A; class-B is ≥2 mechanisms
**Method that finally worked: opt-bisect.** Captured the FAILING fn's real subprocess IR via
`AZ_REMILL_KEEP_SCRATCH=1` (the `{stem}.linked.ll` pre-opt + `{stem}.opt.ll` post-opt), then
`opt -O2 -opt-bisect-limit=N` binary-search on a dup-store heuristic (two adjacent guest
`store volatile i64` with the SAME value = a multi-word value mis-collapsed).
- **MECHANISM A (FIXED, committed dd055272e)**: `-O2` pass #560 **EarlyCSE** collapses two guest
  stores that must hold different register values (a &str fat-pointer ptr@sp+472 + field@sp+488)
  into one. CAUSE: remill tags State-register vs guest-memory as mutually-noalias
  (`!alias.scope !0`/`!noalias !3`) — sound on HW (disjoint spaces), UNSOUND on wasm (one linear
  memory, 32-bit-truncated guest ptrs can hit the State struct). EarlyCSE forwards a non-volatile
  register load across volatile guest stores. FIX = `strip_alias_scope_metadata()` on the linked
  IR before opt (default-on, `AZ_KEEP_ALIAS_SCOPE=1` to keep; LIFT_CACHE_VERSION→4 + key folds the
  toggle). VERIFIED RUNNING: 0/1795 served `.opt.ll` retain `alias.scope`. ⭐ This explains why
  the prior `AZ_NO_HOST_SCOPE` dead-ended — it only stripped tag_state_accesses's 90004/5, never
  remill's native !0/!3 that EarlyCSE actually trusted.
- **MECHANISM B (OPEN, the residual hello-world trap)**: a 16-byte multi-word VALUE's 2nd word
  drops on READ/RETURN — survives the strip (alias-metadata-independent). Witnesses: getHitNode
  (DomNodeId x0:x1) and **AzString `.as_str()`**: hello-world's NodeType::Text AzString feeds
  `text.as_str()` a len of 0xa294828 (= heap_base 0xA000000 + 0x294828 = a HEAP POINTER in the
  len slot) → text3 create_logical builds a 170MB &str → Vec grow → finish_grow OOB trap. NOT
  EarlyCSE. Suspect: wasm multi-value/struct ABI lowering in llc, or GVN/DSE of the {ptr,len}
  load pair. NEXT: AzString-raw-word probe @0x607E0..F0 (window.rs collect_inline_content_for_node)
  splits corrupt-at-build (the 16-byte STORE when the cb built the AzString) vs as_str-misreads
  (the 16-byte LOAD) → then opt-bisect THAT fn's IR the same way (dup-store OR a dropped 2nd load).
- Artifacts: scripts/classB_artifacts/{build_compact,create_logical,reorder}.{opt,linked}.ll
  (gitignored). Re-capture per-fn via the KEEP_SCRATCH recipe (FINAL STATUS above).
## 🎯🎯 MECHANISM-B SHARPLY LOCALIZED to join_generic_copy's String sret return — ptr↔len SWAPPED (2026-06-11 ~18:40, Fable, cron wake)
**The String built by `<...>.collect::<Vec<_>>().join(" ")` (var `collapsed` in split_text, fc.rs
~8723) comes back with its ptr and len fields SWAPPED.** Probe (plain full-opt relift):
`collapsed.as_ptr()==1` (== the real length of "5"!) and `collapsed.len()==0xa294828` (== a heap
pointer). So word0 (ptr slot) holds the length and word2 (len slot) holds the pointer — a clean
ptr↔len swap. `final_text` (= String::new()+push_str(&collapsed)) inherits it → StyledRun.text →
create_logical 170MB slice → OOB. The culprit is `_ZN5alloc3str17join_generic_copy17h...` (a
split_text callee, returns String via sret) — its lift writes the sret String's {ptr,cap,len}
words in the WRONG ORDER (or a single-element join fast-path mis-lifts).
**RESULT: `AZ_LOWOPT_FNS=join_generic_copy,split_text` STILL corrupt** (collapsed.len==0xa294828,
ptr==1). ⇒ join's FAITHFUL lift swaps the sret String's ptr↔len (9 hypotheses now incl. join
opt/llc). Standalone-lifted join_generic_copy (h9c9d2f7abfe94f50 @0xbb15ec → /tmp/join.ll, 0
__remill_error). ⚠ But standalone may NOT reproduce a context-dependent swap (the NEON copy also
lifted correctly standalone yet wasn't reproducible-in-isolation). The swap could be in: (i) join's
sret String stores (word order), (ii) split_text's READ of `collapsed` (both O0, so faithful), or
(iii) the sret-dest (x8) setup at the call site.
**NEXT (precise):** (1) Probe `collapsed`'s RAW words (word0/1/2 via `&collapsed as *const String
as *const u64`) to confirm the in-memory layout is literally {1, ?, 0xa294828} (vs an accessor-read
swap). (2) Capture join_generic_copy's BUNDLE via AZ_REMILL_KEEP_SCRATCH=1 (find __az_dep_<join_addr>
.linked.ll) and examine its sret String stores IN CONTEXT — look for the 3 writes (ptr@+0, cap@+8,
len@+16) to the sret/X8 pointer and whether their values/offsets are swapped. (3) If join is clean
in its bundle, the swap is at the call site (sret dest setup) in split_text — examine that.
A single-element join (`["5"].join(" ")`) may hit a fast-path (return the element / extend) that
mis-lifts. Fix lift-side (remill sret semantics or the specific store).
**NEW probes (committed next): collapsed.len/ptr @0x60C5C/60, push-site id/len/ptr @0x60C50/54/58.**
Fix will be lift-side (dll/src/web or remill fork). If join's sret stores are reordered by the
lift, that's a remill semantics issue (like the historic sret class); if opt, opt-bisect it.

## 🧭 (earlier this wake) MECHANISM-B narrowed: NOT split_text opt/llc, NOT NEON copy (2026-06-11 ~18:20, Fable, cron wake)
**Decisively narrowed this wake (8 relifts). The corruption is in the FAITHFUL remill lift, not LLVM.**
Built a per-fn opt-bisect harness (committed 2fa7b96b3): `AZ_BISECT_FN=<substr> AZ_BISECT_LIMIT=N`
appends `-opt-bisect-limit=N` to ONLY that fn's opt (cache-keyed → only it re-lifts). Plus
`AZ_LOWOPT_FNS=a,b,c` (opt -O0 AND llc -O0 per fn) and `AZ_FULL_CS_RESTORE` (X19-X28 uncond restore).
Tests on split_text's output marker 0x60C44 (==0xa294828 = corrupt, ==1 = clean):
- `AZ_BISECT_LIMIT=0` (split_text, 0 opt passes, 3.4MB unopt bundle) → STILL corrupt.
- `AZ_LOWOPT_FNS=split_text` (opt+llc O0) → STILL corrupt ⇒ not split_text's opt/llc.
- `AZ_LOWOPT_FNS=split_text,grow_one,do_reserve_and_handle` → STILL corrupt.
- `AZ_LOWOPT_FNS=split_text,grow_one,do_reserve_and_handle,spec_from_iter,to_vec,copy_from_slice`
  → STILL corrupt ⇒ not ANY of these at opt/llc.
- `AZ_FULL_CS_RESTORE=1` → no effect ⇒ not a callee-saved-GPR clobber.
- Standalone-lifted split_text's NEON `ldp q2,q3,[x9,#-0x20]; ldp q4,q5,[x9],#0x40` (post-index) AND
  `stp q2,q3,[x10,#-0x20]; stp q4,q5,[x10],#0x40`: BOTH lift CORRECTLY (4×64-bit reads/writes +
  x9/x10 += 0x40). split_text preflight has 0 `__remill_error` (fully decoded). So the 16-byte
  NEON copy is NOT the bug.
**8 HYPOTHESES RULED OUT**: as_str 16-byte return; create_logical OR-pattern; EarlyCSE/alias-scope;
callee-saved-GPR clobber; split_text opt; split_text llc; grow_one/reserve opt/llc; String-build
callees (to_vec/spec_from_iter/copy_from_slice) opt/llc. ⇒ The faithful remill lift of SOME
correctly-DECODED instruction in this path has WRONG semantics (a mis-lifted-but-not-errored op),
OR a callee not yet lowered (e.g. `<str>::to_owned`, the String sret return, the InlineContent enum
store). The String at result[0].text genuinely holds len=0xa294828/ptr=0xa294870 in memory
(consistent across 3 reader fns), built from a sane len-1 &str.
**NEXT:** (a) Confirm faithful with global `AZ_OPT_LEVEL=0` (⚠ huge/slow wasm — may not load; try
once). (b) Examine split_text's faithful IR (scripts/classB_artifacts/split_text.linked.ll OR
standalone /tmp/split.ll) for the `result.push(InlineContent::Text(StyledRun{text:to_string()}))`
region — the StyledRun.text len store + the String sret return from to_owned/to_string. (c) Probe
the to_string() result's len right after the call inside split_text (accept heisenbug risk). (d)
Lower the remaining String path: `to_owned`, `to_string`, `String` ctor, the enum store memcpy.
Markers live: split output 0x60C40/44/48, create_logical entry 0x60C20, fc sites 0x60C00..14.

## 🎯 (earlier this wake) MECHANISM-B PINNED to split_text_for_whitespace's result-Vec buffer (2026-06-11 ~17:40, Fable)
**The corruption is INSIDE split_text_for_whitespace** (fc.rs:8552), confirmed by probing its OWN
output right before `result` is returned (probe 0x60C40/44/48): `result.len()==1` (CORRECT) but
`result[0].text.len()==0xa294828` (heap ptr) and `result[0].text.as_ptr()==0xa294870` (heap ptr,
0x48 away). Input was SANE (fc site-3 as_str().len()==1). So: split builds the StyledRun from a
sane &str, but `result[0]`'s bytes are pointer-garbage while `result.len` is right ⇒ **`result`'s
BUFFER POINTER (result.ptr) is wrong** — `result[0]` (= result.ptr[0]) reads the wrong heap
memory. split_text's hot calls are ALL RawVec grow: do_reserve_and_handle×24, grow_one×14,
handle_error×12 — the machinery `result.push(InlineContent::Text(StyledRun{text:slice.to_string()}))`
runs through. So **grow_one/do_reserve_and_handle mis-sets result.ptr** (the realloc-return / Vec
ptr write-back mis-lifts) in split_text's context.
RULED OUT (4 hypotheses, each a relift): as_str/16-byte-slice-return (AzString sane);
create_logical OR-pattern (standalone if-let reads same garbage); EarlyCSE/alias-scope (survives
strip); callee-saved-GPR-clobber across wrapped call (AZ_FULL_CS_RESTORE=1 no-op — added as an
env-gated diag, keyed into the obj cache). NOT a dup-store (two DIFFERENT pointers).
**NEXT:** opt-bisect split_text's captured bundle (scripts/classB_artifacts/split_text.{opt,linked}.ll)
— focus on grow_one/do_reserve_and_handle's realloc-return handling + the result.ptr write-back;
find the pass that mangles result.ptr, OR examine the linked.ll (faithful) realloc-return store vs
opt.ll. The realloc return is a SINGLE-reg ptr (x0), so it's NOT the x0:x1 16-byte case — likely a
ptr write-back store or a SROA/GVN mis-transform on the RawVec fields. Probes live: split output
0x60C40/44/48, create_logical entry 0x60C20, fc sites 0x60C00..14, AZ_FULL_CS_RESTORE diag.

## 🔬 MECHANISM-B (earlier this wake) — Vec/RawVec growth in split_text_for_whitespace (2026-06-11 ~17:15, Fable)
Cron 92e18a26 (fires :13/:43) is LIVE for the autonomous loop. Findings this wake (committed
75e1bea59, diag probes [az-diag REVERT]):
- **as_str/16-byte-slice-return RULED OUT.** At fc.rs collect_and_measure (all 3 text sites),
  the DOM AzString is SANE: {ptr=heap, len=1, cap=1}, as_str().len()=1 for the "5" counter
  (probes 0x60C00..14). So the bug is NOT the AzString nor as_str's 16-byte return.
- **Corruption is split_text → create_logical.** At create_logical_items ENTRY, via a STANDALONE
  single-disc if-let (probe 0x60C20), content[0].text.len() ALREADY = 0xa294828 (a HEAP POINTER:
  heap_base 0xA000000 + 0x294828), text.as_ptr()=0xa294870 (another heap ptr, 0x48 away). So the
  StyledRun.text String's {ptr,len} BOTH hold heap pointers. NOT the OR-pattern (standalone reads
  same garbage). TWO DIFFERENT pointers ⇒ NOT a dup-store ⇒ EarlyCSE/alias-strip heuristic
  can't catch it (survives strip; 0/1799 opt.ll retain alias.scope).
- **LOCALIZED:** split_text_for_whitespace's hot calls are ALL Vec/RawVec growth —
  do_reserve_and_handle×24, grow_one×14, handle_error×12 — which .to_string() + the result
  Vec<InlineContent> push run through. The g127/g129 "Vec-return len mis-lift" family. Bundle
  captured: scripts/classB_artifacts/split_text.{opt,linked}.ll + create_logical_v2.opt.ll
  (gitignored). split_text root = __az_dep_<split_text_addr>; create_logical root =
  __az_dep_<create_logical_addr>; both their own bundles.
- **Heisenbug CONFIRMED:** a probe added INSIDE layout_flow moved the trap EARLIER (unreachable
  before layout_flow, vs OOB-in-reorder without it) — barrier-defeatable optimizer mis-transform
  per g189. So runtime-probe bisection is unreliable; use OFFLINE opt-bisect (the method that
  cracked mechanism A).
- **NEXT (3 attacks, in order of leverage):** (1) opt-bisect split_text's captured bundle with a
  RUNTIME String-correctness check per bisect-limit (compile→mini→drive; heavy but decisive — the
  static dup-store heuristic does NOT catch mech-B). (2) Audit enforce_sp_preservation coverage of
  split_text's grow_one/do_reserve_and_handle calls — the g129 root was an indirect call
  clobbering X19 (sret/dest ptr); check for an unwrapped call or incomplete saved-reg set near the
  String/StyledRun construction. (3) g189 volatile-barrier: emit a barrier after each guest store
  in the construction region. RULES: lift-side fixes only (dll/src/web or remill fork), NOT
  azul-source out-param hacks. ⚠ DISK: purge azul-web-transpiler-* + az-lift-cache + target/debug
  when free < 8-9 GiB (ENOSPC → 8-byte-stub → empty mini = false "broken").

## (earlier note) MECHANISM-B CONTENT PATH probe history
- ⚠ MECHANISM-B CONTENT PATH IS ELUSIVE: probes at BOTH
  window.rs collect_inline_content_for_node::Text (0x607E0..F0) AND
  sizing.rs collect_inline_content_recursive (0x60714/18/1C/54, prior-session markers) read 0
  — neither fired — yet create_logical_items::push (0x607C8) DID see the garbage len 0xa294828.
  ⇒ the text reaches create_logical via a THIRD path (suspect: content collected once in the
  sizing/intrinsic-width pass, CACHED, then re-fed to layout_flow; OR create_logical called from
  a path that synthesizes content without those collectors). NEXT: find create_logical_items's
  actual caller in the layout_flow/layout_ifc chain (grep callers; the trap stack was
  finish_grow←reserve←reorder_logical_items←layout_flow←layout_ifc←layout_formatting_context),
  probe THERE for the AzString {ptr,len}, then opt-bisect that fn. Mechanism B is the ONLY thing
  between here and hello-world green on the rebased engine.

## (superseded by the above) FINAL STATUS (2026-06-11 ~08:10) — CONSOLIDATED, cron `f1b4a997` DELETED, decision point for user
**Shipped + green (all committed):** S0 (preflight + lift/obj cache), remill NZCV decoder
(fork 70050e0), S1 (every JS input event → wasm cb, CDP-proven), generic byte-image hydration,
shared bump heap + cache-v3. hello-world (counter 5→6) and web-events-min (click/keydown/resize)
are CDP-green. The web backend runs real lifted azul for small apps.

**The one gate for S2–S7 (and any app >~6 nodes): class-B = lifted multi-word value loses its
2nd word.** Two faithful-standalone-but-broken-in-pipeline witnesses: `getHitNode` (16-byte
DomNodeId returned in x0:x1 → node/x1 half drops → S2's `set_css_property` panics on node=0) and
`build_compact_cache_with_inheritance_debug` (176-byte CompactLayoutCache via X8/sret → tier2b
Vec header arrives {ptr=garbage, len=0, cap=19✓} → 19-node web-events traps in collect_font_stacks).
The CSS-output API *intrinsically* passes DomNodeId by value, so class-B is NOT bypassable by a
web-only shim if real apps are to work unchanged.

**🔑 NEW THIS WAKE — why every prior IR-dump failed + the artifact is finally captured:**
- The real compile pipeline is the **SUBPROCESS** `llvm-link → opt → llc` (LLVM-21 tools), NOT
  the in-proc `compile_inner` in `dll/src/web/cpp/azul_remill.cpp`. In-proc is gated by
  `AZ_NATIVE_REMILL` (default OFF, "subprocess is the default", transpiler_remill.rs:663-666).
  So the `AZ_DUMP_OPT_IR` dump added to azul_remill.cpp **never ran for the real path** — it only
  ever caught the in-proc dispatcher. That dead scaffolding is now REVERTED (azul_remill.cpp ==
  HEAD again).
- The real per-fn post-opt IR is written by the subprocess as `{stem}.opt.ll` (and `.linked.ll`
  = pre-opt) in the transpiler scratch dir, normally cleaned on Drop. **`AZ_REMILL_KEEP_SCRATCH=1`
  retains it** (transpiler_remill.rs:634-639, 2771).
- **CAPTURE RECIPE (one relift, ~250s, no code change):**
  `AZ_NO_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1 bash scripts/web_relift.sh examples/c/web-events.bin /tmp/x.log`
  → kept scratch path is logged ("keeping scratch dir …azul-web-transpiler-<pid>"). build_compact's
  per-run lifted name is `__az_dep_<hex>` where hex = its native addr from the log line
  `lifting …build_compact_cache_with_inheritance_debug… export_as=__az_dep_XXX`. It compiles as its
  OWN bundle: `__az_dep_XXX.opt.ll` (1.6 MB, defines it) + `.linked.ll` (pre-opt).
- **ARTIFACT PRESERVED** (this run, build_compact = `__az_dep_10b631d64`):
  `scripts/classB_artifacts/build_compact.opt.ll` (post-opt) + `build_compact.linked.ll` (pre-opt).
  ⚠ run-specific SSA names/addrs — a future session should regenerate fresh via the recipe; these
  are for the immediate next pass. (Untracked; not committed — 4 MB of run-specific IR.)

**Where the deep fix work resumes (focused session, NOT 30-min cron wakes):** in
`build_compact.opt.ll`, build_compact's body = lines ~14473–27928 (returns `i32 %ret_w`), 672
`store volatile i64`. The 176-byte sret return-copy writes the struct to the X8 dest (X8 =
`getelementptr i8, ptr %state, i32 672`; its loaded value is the dest base). tier2b lands at struct
+0x48(cap=72)/+0x50(ptr=80)/+0x58(len=88). Candidate +72/+80/+88 store clusters: 19784, 20579,
21332, 22274, 23149, 24018, 25025, 25627, 27778 — but most use loop-phi/local bases (e.g. 25627's
base %3055 is a loop phi, NOT the sret dest). **Next step is mechanical, not manual-read:** isolate
the cluster whose store-base traces to the X8-loaded value, then diff that block pre-opt
(`.linked.ll`, the faithful lift) vs post-opt (`.opt.ll`) — if the tier2b ptr/len stores are
present+correct in pre-opt and dropped/mis-sourced post-opt ⇒ opt pass (pin/disable/barrier in the
subprocess `opt` flags, `opt_flag_for`/`buildPerModuleDefaultPipeline`); if present+correct in BOTH
⇒ llc wasm codegen / FIX_SP textual pass (enforce_sp_preservation runs on `.opt.ll` after opt).
RULED OUT across this+prior sessions: instruction decoders (str q / ldur q / getHitNode all
faithful standalone), opt level (-O0 on the fn), alias scopes (AZ_NO_HOST_SCOPE), FIX_SP-off (breaks
earlier), from_elem construction, AZ_FUEL barriers, isolated full-pipeline on LLVM-21 (preserves x1).

**DECISION POINT (why the loop stopped here, per this file's pre-registered STOP CONDITION):**
class-B is the prior session's known-hard bug; many wakes have characterized it exhaustively but the
remaining fix is a sustained focused-session effort (now unblocked by the captured artifact), not
fragmented 30-min autonomous wakes. **User chooses:**
 1. **Invest in the deep class-B fix** — resume from `scripts/classB_artifacts/` + the recipe above;
    one engineer-session of opt-pass bisection on the captured IR. Cracking it unblocks S2–S7 +
    all large apps at once (and likely many "layout mismatches" that are really this mis-lift).
 2. **Ship the green baseline as-is** — S0/S1 + shared-bump are a usable web backend for small
    apps; accept that multi-word-struct callbacks (CSS-out, etc.) wait on the class-B fix.
 3. **Redirect** — e.g. pursue ua_css Chrome-parity (S7, independent of class-B) or a different
    slice while class-B is parked.
To resume the loop, just say which; the superplan + artifact make pickup immediate.

---

## Slices (user's order, increasing complexity)

- **S0 — infra (in flight)**
  - [x] Engine-aware lift cache (`AZ_LIFT_CACHE=1`, key = fn-bytes + lift-addr + version +
        remill-binary fingerprint). Baseline cold relift = **250 s** (was 15-30 min claim).
  - [x] Preflight clean-lift gate (`AZ_PREFLIGHT=1`): reports per-fn `__remill_error` (undecoded
        instr) + `__remill_missing_block` counts at startup. **Found 22 fns / 27 error sites**
        (core smallsort ×4, Vec::from_iter ×2, Map::fold ×2, taffy grid+flexbox ×5, solver3 fc ×5,
        positioning ×2, allsorts cmap/woff2 ×2, FcFontCache::get_font_bytes,
        layout_dom_recursive, build_compact_cache_with_inheritance_debug).
  - [x] **Undecoded instructions identified + FIXED**: triage (scripts/web_lift_triage.py —
        nm addr → extract fn bytes → standalone remill-lift-17, stderr names the instr) showed
        only ONE real gap across all 22 fns: `mrs/msr NZCV` (flag save/restore in Rust's
        smallsort networks). Implemented in the remill fork (Arch.cpp SystemReg + SYSTEM.cpp
        semantics, commit 70050e0 — bundled with the prior session's FP batch FRINTM/N/P/Z,
        FSQRT, FABD, FCMGT, FCVTN). All 22 fns now standalone-lift with ZERO decode errors.
        Residual __remill_error sites = fall-through blocks after noreturn `bl panic` at byte-
        range end — unreachable, benign. Triage rule: stderr-silent = benign; stderr-noisy = real.
  - [x] Commit S0 (azul-mobile 4-file commit) + remill fork 70050e0.
  - [ ] Preflight v2 (optional): auto-classify the benign noreturn-tail class so the report
        only screams on real gaps.
  - [ ] **Cache-v2 (MEASURED)**: lift-cache misses 100% across code-moving dylib rebuilds
        (789/789) — key + IR embed layout-derived synth addrs (symbol_table.rs:627). Same-dylib
        re-lifts hit 100% (hello-world regression run: 0 writes). BUT a `sample` of a cache-hot
        run showed the REAL bottleneck: `produce_object_from_lifted_ir → run_tool(llvm-link|
        opt|llc)` — 3 subprocess spawns per fn, run SEQUENTIALLY per BUNDLE walk, and the
        per-bundle closure walks recompile the same shared deps up to N+2 times (mini + layout
        + 9 cbs ≈ 4× hello-world's 3 bundles ≈ the observed 4× slowdown).
  - [x] **cache-v2a: content-keyed OBJECT cache** (obj_cache_path in transpiler_remill.rs):
        key = patched IR + helper IR + opt flag + IR-mutating env toggles + engine fingerprint;
        fn-matched diagnostic envs (AZ_REG_TRACE etc.) disable caching for that fn. Collapses
        the cross-bundle recompiles within ONE relift AND across relifts of an unchanged dylib.
        Verify with the events4 relift timing. NOTE: azul-source edits still cold-start both
        caches (synth churn) — stable name-keyed synth stays the (deferred, risky) endgame.
  - [ ] **AZ_NATIVE_REMILL=1 experiment** (in-proc LLVM-17 compile, no subprocesses; currently
        OFF and mutually exclusive with the lift cache, line 760): measure + e2e-validate on
        hello-world AFTER the trap fix. Caveat: in-proc is vcpkg LLVM-17, subprocess tools are
        LLVM-21 — different opt pipelines, needs its own green run before flipping defaults.

- **S1 — INPUT events: every JS event reaches its wasm callback (CDP-proven)**
  - Have: click, mousedown/up, dblclick, keydown/up, focusin/out, input(text), scroll, resize
    listeners + encoders; per-node kind filter (`cb_node_kinds[64]`, data-az-ev).
  - [x] IMPLEMENTED (pending relift+CDP verify): mousemove (rAF-throttled), wheel (→ EVT_SCROLL
        at pointer, desktop parity), mouseenter/mouseleave (capture-phase, DOM-TARGET routed —
        leave coords lie outside the node, never hit-test), contextmenu (preventDefault only on
        registered targets). event_kind += MOUSEENTER 12 / MOUSELEAVE 13 / CONTEXTMENU 14.
  - [x] Broadcast routing: RESIZE/SCROLL/KEYDOWN/KEYUP dispatch to EVERY node registered for
        that kind (azul Window-filter semantics), never bbox-hit-test. RESIZE records
        viewport_w/h in EventloopState. focused_node_idx tracked from FOCUSIN/FOCUSOUT
        (Focus-filter keyboard precedence deferred to S2).
  - [x] html_render: EventFilter::Window(...) arm added to event_filter_to_js_name (resize cbs
        previously registered as "click"!), Hover(RightMouseUp) → "contextmenu";
        azEvNameToKind learned mousemove/mouseover/mouseenter/mouseleave/contextmenu/input/resize.
  - [x] azDispatchTargeted + azNodeIdxFromTarget (az_N from DOM target) for focus/enter/leave/key.
  - Multi-cb-per-node is OUT of scope v1 (cb_fn_cache is one-cb-per-node); test app uses one
    node per event kind.
  - [x] **VERIFIED + COMMITTED** (2026-06-11): web-events-min.c (~5 nodes, under the class-B
        trap) CDP-green — click (single-target hit-test) + keydown + resize (Window broadcast)
        all reach their wasm cbs (counters 0→1/0→1/0→2). Commits 43b2a455f (core web_lift +
        get_data_len), 3438d602e (S1 routing + hydration), 4091af06d (obj-cache + store-log
        windowing), bfab10060 (test apps/harnesses/docs). web-events.c (19 nodes) still traps
        on class-B — kept as the reproducer. Post-revert regression re-confirmed green.
  - NOTE: full 9-kind web-events.c verification (mousemove/wheel/enter/leave/contextmenu)
    is BLOCKED by class-B (needs ≥19 nodes). Re-run scripts/m9_e2e/web-events-cdp.js once
    class-B is fixed to close S1 100%.

- **S2 — OUTPUT: callback modifies CSS, wasm cascades, TLV sets inline attrs**
  - The bug: `AzStartup_dispatchEvent` passes raw event bytes as the cb's `info` ptr
    (eventloop.rs:2126) — every AzCallbackInfo_* call today reads garbage.
  - [ ] Real `CallbackInfo` wasm-side: `changes: Arc<Mutex<Vec<CallbackChange>>>` lives in
        EventloopState (init like cb_fn_cache); ref_data → zeroed blob (grown later);
        hit_dom_node = {dom:0, node:node_idx+1}; cursors from event x/y. AzCallbackInfo_* bodies
        are GENERATED (target/codegen/dll_api_internal.rs, include!d in dll) → lift inside each
        cb closure automatically — no per-API wiring.
  - [ ] Drain after cb returns: lock + iter + clear IN PLACE (`take_changes()` returns Vec by
        value = OPEN class-B sret bug — avoid). Translate: ChangeNodeCssProperties /
        OverrideNodeCssProperties → `CssProperty::format_css()` (css/src/props/property.rs:4545,
        same fn html_render uses) → PATCH_KIND_SET_INLINE_STYLE; ChangeNodeText → SET_TEXT;
        SetFocusTarget → FOCUS; ScrollTo → SCROLL_TO. Apply the change to the wasm-side styled
        state too (wasm stays authoritative; cascade equality with desktop).
  - [ ] JS `azApplyPatches` case 4: switch from `setAttribute('style', …)` (clobbers
        server-rendered inline styles) to per-declaration `el.style.setProperty/removeProperty`.
  - [ ] Test: `examples/c/web-setcss.c` — click → `set_css_property(background-color + width)`;
        CDP click → assert `el.style.backgroundColor`.

- **S3 — TIMERS (drive animations)**
  - [ ] `Instant::now` HostCall injection at IR layer (clock = `performance.now()`); lifted
        Instant::now returns 0 on wasm → timers never advance without this.
  - [ ] Drain AddTimer/RemoveTimer changes → new TLV kinds (13 AddTimer {timer_id, tick_ms},
        14 RemoveTimer) → JS `setInterval`/`clearInterval` → each tick calls new export
        `AzStartup_tickTimer(state, timer_id)` → invokes the timer's cb (timer cb fn-ptr needs a
        pre-lifted per-cb wasm: server must lift-seed timer callbacks found in the binary — the
        fn-ptr lift-seed groundwork from commit 953162fa7) → drain changes → patches back.
  - [ ] Test: `examples/c/web-timer.c` — timer animates a div's width via set_css_property;
        CDP: watch style change over ~10 ticks.

- **S4 — IMAGES + FONTS (dynamic)**
  - Fonts already load via `/az/` URLs from the server (see loader_js azBootstrap font fetch +
    AzStartup_setFallbackFont) — extend to runtime-added fonts.
  - [ ] Image output: ChangeNodeImage/AddImageToCache/RemoveImageFromCache changes → TLV kinds
        (15 SetImage {node, src-bytes or /az/img/<id> URL}, …) → JS sets `<img src>` /
        background-image; RGBA buffers → canvas/ImageBitmap → blob URL.
  - [ ] Image callbacks (RenderImageCallback) + UpdateImageCallback/UpdateAllImageCallbacks →
        re-invoke image cb wasm → RGBA out → canvas. Enables infinite scroll demos.
  - [ ] Test: image swap on click + an image-callback node rendering a gradient buffer.

- **S5 — THREADS (async.c on the web)**
  - [ ] AddThread change → TLV → JS spawns a dynamically generated Web Worker whose shell loads
        the thread-cb wasm (server pre-lifts `ThreadCallbackType` fns same as timer cbs);
        ThreadSender/ThreadReceiver ↔ postMessage; writeback cb runs on main; worker close →
        RemoveThread. `std::thread::spawn` does NOT lift — the web-native create_thread is the
        IR-injected HostCall.
  - [ ] Target: `examples/c/async.c` (or its web twin) works end-to-end.

- **S6 — AzHttp → fetch**
  - [ ] Detect `AzHttp_*` symbols in the lift closure → FnClass::HostCall → JS `fetch()`;
        async result posts back as an event/TLV into wasm (same inbound path as DOM events).
  - [ ] Test: http demo fetches a URL and displays status/body length.

- **S7 — ua_css.rs Chrome parity**
  - [ ] Diff `layout/src/…ua_css…` against Chrome's html.css defaults (margins on body/h1-h6/p,
        display types for table/list elements, form-control styling); align so unstyled content
        renders like Chrome.

## Key architecture facts (verified 2026-06-11)
- `AzCallbackInfo_*` = generated transmute wrappers → `azul_layout::callbacks::CallbackInfo`
  methods → `push_change(CallbackChange::…)`. In the dylib ⇒ lifted automatically per-cb.
- `CallbackInfo` repr(C) (layout/src/callbacks.rs:741): {ref_data*, hit_dom_node: DomNodeId,
  cursor_relative_to_item, cursor_in_viewport: OptionLogicalPosition, changes:
  *const Arc<Mutex<Vec<CallbackChange>>>} (std build).
- `CallbackChange` (layout/src/callbacks.rs:167): ~60 variants — the natural patch boundary.
- TLV transport complete (kinds 1-12 both sides); `AzStartup_buildPatch` exists, unused.
- Dispatch flow: JS listener → 256-byte event buffer → AzStartup_dispatchEvent → hitTest
  (positioned_rects) → kind filter → __az_resolve_callback → __az_call_indirect(table_idx,
  refany_lo, hi, info_ptr) → per-cb wasm `callback` export.
- Per-cb wasms share memory+table with mini; bootstrap instantiates per `[data-az-cb][data-az-wasm]`
  element; `data-az-ev` → kind registration (html_render.rs:388-405).
- Mutex = macOS os_unfair_lock = out-of-image → Leaf no-op stub = correct for single-thread wasm.
- Relift loop: `bash scripts/web_relift.sh examples/c/hello-world.bin /tmp/server.log` (bg,
  poll 8800; 250 s cold, less warm). CDP: Chrome on :9222, /tmp/cdp_drive.js + cdp_probe.js +
  cdp_rects.js. Disk: `df -h /`, purge `/var/folders/*/T/azul-web-transpiler-*`.

## Session log
- **2026-06-11 ~08:10 (consolidation, cron deleted):** Found the root cause of every failed
  class-B IR-dump: real pipeline = subprocess LLVM-21, not in-proc `compile_inner` (AZ_NATIVE_REMILL
  default-OFF). Reverted the dead in-proc `AZ_DUMP_OPT_IR` scaffolding (azul_remill.cpp == HEAD).
  Captured build_compact's real post-opt + pre-opt IR for the FIRST time via
  `AZ_REMILL_KEEP_SCRATCH=1 AZ_NO_LIFT_CACHE=1` relift → preserved to `scripts/classB_artifacts/`.
  Per the pre-registered STOP CONDITION (body captured, but the exact fix is a focused-session task
  not a cron wake): consolidated + surfaced the decision point (see FINAL STATUS at top). Cron
  `f1b4a997` deleted.
- **2026-06-11 (overnight, autonomous):**
  - S0: preflight+cache shipped+committed; NZCV decoder fixed in remill 70050e0 (the only real
    decode gap of 22 flagged; rest = benign noreturn-tail artifacts; triage script saved).
  - Generic byte-image hydration (size+bytes in az-hydrate; RefAny::get_data_len added) —
    legacy path alloc'd 4 bytes for EVERY model; >4B models corrupted the bump heap.
  - S1 implemented (kinds 12-14, broadcast routing for RESIZE/SCROLL/KEY*, focus tracking,
    targeted dispatch, html_render Window-filter names, rAF mousemove, wheel→scroll).
  - hello-world REGRESSION GREEN on new engine (counter 5→6 via CDP).
  - web-events TRAP bisected: `unreachable` in lifted collect_font_stacks_from_styled_dom;
    NATIVE repro test (layout/tests/web_events_repro.rs) PASSES ⇒ MIS-LIFT confirmed (user
    called it). Marker bisection (cdp_patch_probe): pre-loop markers set, post-phase-1 tag
    unset, bump allocator healthy ⇒ trap = tier2b_text[i]/tier1_enums[i] slice bounds panic
    ⇒ lifted build_compact_cache produced SHORT arrays (or Box deref wrong). Probe markers
    0x606A0-AC added (lens + Box addr + node_count) — relift events4 in flight.
  - Object cache (cache-v2a) implemented same rebuild.
  - User directives: many "layout bugs" are mis-lifts (confirmed); compare native vs lifted
    against azul-doc reftests as the systematic shaker (TODO: lifted-reftest runner — one
    binary, model carries the test xht via generic hydration, diff positioned_rects native vs
    lifted per test).

## Trap root-cause trail (live, 2026-06-11 ~02:30)
- Probe verdict: tier2b_text = (cap=19 ✓, ptr=3 ✗, len=0 ✗); tier1 fully correct; compact Box
  sane. The **minimal sret witness PASSES** (len=7 ✓) — NOT a blanket class-B failure.
- Producer found: `CompactLayoutCache::with_capacity` (css/src/compact_cache.rs:1466, native
  0xf0da5c) — real call from build_compact_debug, returns the whole 5-Vec struct via sret
  (x8→x19). Native tail (f0ddf0-f0de48) stores 21 fields straight-line: tier2b cap=x26@+0x48
  (LANDS lifted), ptr=x25@+0x50 + len=x20@+0x58 (GARBAGE lifted). x25/x26 are set by the
  3-mov chain right after the 3rd `bl __rust_alloc` (f0dd3c-44: mov x13,x25; mov x25,x0;
  mov x26,x20).
- **Raw remill IR of the whole fn is FAITHFUL** (standalone-lift: tail stores X26/X25/X20 →
  X19+72/80/88 correct; post-alloc mov chain correct; 0 decode errors). ⇒ corruption enters in
  the REAL pipeline's transform stack: BumpAlloc helper inlining (its X0 store is NON-volatile
  byte-GEP state+544 with HOST metadata from tag_state_accesses, vs guest struct-GEP loads) +
  opt -O2 + FIX_SP — alias/metadata-driven mis-forwarding is the prime suspect.
- Tooling armed: relift events6 running with AZ_LOG_STORES=CompactLayoutCache13with_capacity
  AZ_LSWIN_HI=0x7000000 (M12.5y store ring @0x41000/0x41010+k*16); /tmp/cdp_patch_probe.js
  now dumps the ring post-trap.

## Trap hunt II (events6-9, ~02:35-03:10)
- **with_capacity EXONERATED by ring**: its lifted tail writes the FULL struct correctly
  (tier2b cap=19/ptr=0x6043478/len=19 to sret x19=0x2ebf8). Its fill loops write correct
  sentinels (0x7fff) to the vec buffers.
- **build_compact's body exonerated by logic**: it indexes tier2b_text[i] per node during the
  build — len must have been 19 throughout, else it would have trapped much earlier.
- **Suspect narrowed to TWO copies**: (1) build_compact_debug's return q-copy (d5bbb8-d5bc1c:
  ldur q pairs from its stack → str q through saved-x8 x24; the doomed +0x50 chunk = `ldur q1,
  [sp,#0xe8]; str q1,[x24,#0x50]`), then (2) create_from_compact_dom's 432-byte memcpy
  (LibcMemcpy/llvm.memmove) of the struct into the Box @0x6046b00 (ring8: dest+len logged 2×).
- **Standalone lifts of EVERYTHING are faithful** (with_capacity whole-fn; build_compact
  whole-fn — its only error block is the brk panic via sync_hyper_call; the ldur-q snippet
  lifts with correct offsets 216/232 + i128 composition). ⇒ the corruption is injected by the
  REAL pipeline's per-fn opt stage (alwaysinline helper bodies + alias-scope tagging + FIX_SP
  + selfloop-rewrite over the linked module) — the g188/g189 "barrier-defeatable optimizer
  mis-transform" family — or lives in the memcpy helper body path.
- ⚠ METHOD BUGS FIXED: (a) remill-lift --bytes wants MEMORY-ORDER hex (byte-reversed words) —
  single-word "evidence" earlier was garbage; (b) "stderr-silent error = benign tail" is
  unreliable — silent decode-fail exists. Triage must locate error blocks positionally.
- Tooling: AZ_LSWIN_LO added (transpiler) — windowed store ring [lo,hi). events9 in flight:
  AZ_LOG_STORES=build_compact… LO=0x2e000 HI=0x30000 → only the caller-frame writes (the
  return q-copy's 11 stores) → verdict on hop (1) vs (2). Scratch watcher armed to capture
  __az_dep_*.instr.ll + .opt.ll (scratch is cleaned post-lift — copy DURING).

## Trap hunt III — THE POISON WRITE (events8-11, ~02:55-03:10)
- ring8 (create_from instrumented, complete 1292 entries) contains the corruption being
  WRITTEN: sites wrote (cap=0x13 @0x2f0f8, ptr=3 @0x2f100, len=0 @0x2f108) — the compact
  triple at its stack home (struct base 0x2f0b0) — i.e. create_from_compact_dom's lifted code
  stomps tier2b.{ptr,len} AFTER build_compact returns. build_compact + with_capacity + every
  copy hop are exonerated (ring6/ring9 + faithful standalone lifts).
- ⚠ Ring ids are PER-BUILD (instrumentation numbers post-opt output) — never map one build's
  ids onto another build's IR. events11 (in flight) re-instruments create_from; its ring +
  captured /tmp/cfc_instr.ll are id-consistent.
- NEW HYPOTHESIS from the IR read: the logger also logs State-REGISTER alloca stores (%PC,
  %W3…); their ring addresses reveal where llc placed the module's SHADOW-STACK frames. If
  those allocas land in 0x2fxxx, the lifted module's own stack frames OVERLAP the guest
  memory region where create_from's structs live → every register spill stomps guest data.
  (Would explain heisenbugs galore: g188 "barrier-defeatable", M12.5x self-relative stores.)
- AZ_LSID_LO added (id-floor tail tracing). Source context: after build_compact,
  `compact_cache = Some(compact)` then `prune_compact_normal_props()` then generate_tag_ids.

## ⚠ STANDING BUG found en route (fix with the batch): stack-slot/mirror collision
`relocate_stack_if_non_mini` (transpiler_remill.rs:2838) hands each wasm module SP =
192KiB + slot×128KiB, "below the bump heap at 1 MiB" — but slots 7+ land AT/ABOVE 0x110000,
inside the data-mirror/bump band (0x110000..). web-events has ELEVEN modules (mini + layout +
9 cbs) → slots 7-10's shadow stacks overwrite mirrored data pages. hello-world (3 modules)
never hit it. Fix: move STACK_BASE_FIRST band somewhere safe for ≥32 modules (e.g. carve
below the mirror: raise the bump/mirror base, or stride into a dedicated high band) — needs
the memory-map audit (mirror pages, bump base 96MiB, ring buffers 0x40000-0x50000).

## Trap hunt IV (events11-12) — RED HERRING cleared, timeline bisection armed
- The 0x2f0f8 "triple" was a RED HERRING: ids 592-594 are State-REGISTER GEP stores
  (`store i64 …, ptr %W3` / `ptr %PC`) — `ptrtoint %W3`=0x2f0f8 just means the lifted State
  struct sits there; the (0x13,3,0) values are transient register contents, NOT the compact
  struct. The REAL corruption (confirmed by markers) is in the HEAP Box @0x6046bd8:
  tier2b_text reads cap=19 ✓ but ptr=3, len=0 ✗ (tier1_enums fully correct).
- prune_compact_normal_props EXONERATED (only retain()s css_props/cascaded_props; never
  touches the compact Vecs). So corruption is either build_compact's RETURN (real pipeline,
  not the faithful standalone lift) or the `compact_cache = Some(compact)` move.
- events12 (in flight): source bisection probe in create_from (styled_dom.rs:1086) — 0x60660
  tier2b just after build_compact returns (pre-move), 0x60670 just after the Some-move. Reads
  ptr/len/cap at both. Decides: A bad ⇒ build_compact return mis-lift; A good+B bad ⇒ the move;
  both good ⇒ a later aliasing write (instrument heap window 0x6046000-0x6047000 next).

## ⚠ METHOD FIX (events12 wasted): azul-CORE had NO web_lift feature
- events12's A/B markers read 0 because `#[cfg(feature="web_lift")]` in core/styled_dom.rs was
  DEAD — azul-core never had a web_lift feature; only azul-LAYOUT did (that's why getters.rs
  probes fire but styled_dom.rs ones didn't). FIXED: added `web_lift = []` to core/Cargo.toml,
  and `azul-layout/web_lift` now forwards `azul-core/web_lift` (layout/Cargo.toml:146).
  ⇒ ANY future core probe/shim under web_lift now actually compiles. events13 rebuilds with it.
- Reminder for the diff-vs-reftest plan (user): also lets us add web-only shims in core, not
  just layout.

## ⚠ LESSON (events13 segfaulted): never put a fixed-address marker write in a
## CASCADE function — they run NATIVELY too.
- create_from_compact_dom runs natively in the server's render_initial_page (HTML emit), so my
  `write_volatile(0x60660,…)` probe hit unmapped native memory → SIGSEGV at server start.
  Layout-time fns (getters.rs) are wasm-only so their az_mark is safe; cascade fns are NOT.
- FIX: reverted the styled_dom.rs probe (kept the core web_lift feature — harmless/useful).
  For cascade-stage bisection use TRANSPILER instrumentation (AZ_LOG_STORES, wasm-only by
  construction) instead of source markers. events14 in flight: AZ_LOG_STORES=create_from_compact_dom
  windowed to the heap Box [0x6040000,0x6048000] → see the tier2b triple written into the Box.
- Refined signature: tier2b_text {ptr@0=3 ✗, cap@8=19 ✓, len@16=0 ✗} — 1st+3rd words of the
  24-byte Vec wrong, middle right. with_capacity provably returns it len=19 (vec![default;n]);
  build_compact fills by index (would panic if len<n; native doesn't). So len got ZEROED in
  build_compact's RETURN or the Some-move. Not a clean truncation (cap survived). Smells like
  2 dropped/misrouted stores of a specific field's ptr+len in a lifted sret/memcpy.

## SHARPER LEAD (2026-06-11 ~03:30): the trap is NODE-COUNT dependent
- hello-world (5 nodes, 1 text node) lays out FULLY — its tier2b compact cache is NOT corrupted
  (counter 5→6 verified). web-events (19 nodes, 9 text nodes) traps in collect_font_stacks with
  tier2b {ptr=3, len=0}. ⇒ the mis-lift is exposed by SIZE (more nodes/text), not universal.
  Likely a vec-grow / SROA-threshold / loop-bound path that only triggers above N. This also
  means a MINIMAL repro exists between 5 and 19 nodes → far cheaper to bisect than the full app.
- AZ_LOG_STORES can't see the `Some()` move (it lowers to llvm.memmove, not a plain `store`;
  the log only instruments `store`/memset-len). events14 heap window saw only adjacent css-cache
  vec builds, not the compact copy. ⇒ runtime marker bisection (events15, native-safe probe
  with the <4GiB wasm-address guard) is the right tool.

## ✅ ROOT CAUSE LOCATED (events15, 2026-06-11 ~03:50): class-B LARGE-struct sret
- A_afterbuild = (ptr=0x2, len=0, cap=0x13) — tier2b is corrupt THE INSTANT build_compact
  returns, before the move. cap (2nd word) survives; ptr (1st) + len (3rd) garbage.
- ⇒ build_compact_cache_with_inheritance returns a 176-byte CompactLayoutCache via X8/sret and
  the lifted return-copy corrupts it. THE class-B bug from HANDOFF §2 — but the staged witness
  (24-byte Vec<u64>) was too SMALL to reproduce; this 176B/6-Vec+BTreeMap struct is the real
  reproducer. Standalone lift was faithful ⇒ a REAL-PIPELINE opt transform breaks it (H2 family:
  load-forward / DSE / reorder of guest mem ops across inline-cloned alias scopes — the g188/g189
  "barrier-defeatable" class). Node-count dependent because more nodes = bigger struct/more copy.

## events16 verdict + STRATEGIC PIVOT (2026-06-11 ~04:00)
- events16 (build_compact + with_capacity at -O0): STILL CORRUPT (ptr=0x2, len=0). ⇒ NOT an
  optimizer transform on build_compact's own body. The corruption rides a pass that runs
  regardless: FIX_SP (post-opt textual, events17 testing), the sret call-bridge/X8 threading
  (handoff H3), or the raw lift's internal-sret routing. Static IR reads aren't converging
  (post-opt is fully inlined/renamed; 672 appears as both the X8 State offset AND incidental
  frame offsets).
- **DECISION**: class-B large-struct sret is the prior session's KNOWN-HARD bug (25+ iters,
  handed off unsolved). Solving it properly is a multi-hour remill/transpiler effort. It is
  PRE-EXISTING, not introduced by S1. So: (a) keep bisecting cheaply via env toggles
  (events17 FIX_SP, then AZ_NO_HOST_SCOPE) to localize for a future fix; (b) IN PARALLEL
  unblock the actual deliverable — verify S1 event-routing on examples/c/web-events-min.c
  (1 click div + 2 body-level Window cbs = hello-world-class node count, stays under the
  trap) → commit S1; (c) track class-B as its own deep item.
- web-events-min.c written: single-target hit-test (click) + broadcast (keydown/resize Window
  filters) — the two S1 routing paths — with ~5 nodes.

## class-B PRECISELY LOCALIZED (2026-06-11 ~04:00) + remaining tests
- events17 (FIX_SP off) INCONCLUSIVE: all markers 0 incl w_reached ⇒ pipeline breaks EARLIER
  without FIX_SP (it's load-bearing per M12) — can't use as a class-B diagnostic.
- build_compact return copy = 11× `str q` (NEON 128-bit) covering the 176B struct. Mapping
  (current dylib hb535, fn@0xd59f64): tier2b cap@0x48 rides `str q0,[x24,#0x40]` (LANDS),
  tier2b ptr@0x50 + len@0x58 ride `str q1,[x24,#0x50]` (CORRUPT). x24 = sret dest (mov x24,x8
  at entry d59fc4; x8 = state+672). Same x24 base for both, so NOT a wrong-dest — the q1
  VALUE/transfer is the bug.
- BOTH endpoints lift FAITHFULLY in isolation: `str q1,[x24,#0x50]` (0117803d) → 2× write_memory_64
  at +0x50/+0x58 ✓; `ldur q1,[sp,#…]` → 2× read_memory_64 at correct offsets ✓. And -O0 doesn't
  fix it. ⇒ the bug is NOT the instruction decoders and NOT opt — it's either (a) remill's
  memory-model lowering / alias-scope handling of the q-transfer's HIGH half in the linked
  pipeline, or (b) a body store with a mis-lifted address clobbering tier2b's ptr/len in
  build_compact's LOCAL frame before the return copy reads it (fits node-count dependence:
  more fill iterations = more clobber chances).
- REMAINING cheap tests (need the port, currently held by the S1 min relift): AZ_NO_HOST_SCOPE=1
  (alias-scope off — if it fixes tier2b, the tagging is wrong on the q-pair copy); then
  instrument the LOCAL frame (window build_compact's sp-region) to see if tier2b's local slot
  is correct just before the return copy (distinguishes (a) vs (b)).

## ✅ S1 DONE (committed 2026-06-11 ~04:12). Class-B is now the gating blocker for S2+.
The next slices (S2 CSS-out, timers, images) all need apps richer than 5 nodes → they will
trip class-B. So class-B must be fixed (or stopgapped) BEFORE S2 can be e2e-verified. Two paths:
 - PROPER FIX (preferred): localize via AZ_NO_HOST_SCOPE + local-frame instrument (below),
   then fix in transpiler/remill (alias-tagging on the q-pair sret copy, or the memory-model
   lowering of the q-transfer high half).
 - STOPGAP (if proper fix is multi-session): convert build_compact_cache_with_inheritance to a
   `&mut CompactLayoutCache` out-param (web_lift-gated, documented TODO-remove) — the sanctioned
   class-B workaround pattern from HANDOFF §4.A. Unblocks ALL slices immediately.

## class-B RE-FRAMED (2026-06-11 ~04:20): NOT the return copy — a fill-loop clobber
- AZ_NO_HOST_SCOPE=1: STILL TRAPS ⇒ alias tagging is NOT the cause.
- KEY LOGIC: the return copy is FIXED 176 bytes (struct holds Vec headers, not inline node
  data) regardless of node count. hello-world (5 nodes) works, web-events (19) traps, SAME
  return copy ⇒ the return copy is NOT the corruptor. The local cache must already be corrupt
  BEFORE the return — a NODE-COUNT-DEPENDENT fill-loop store clobbering tier2b's local Vec
  header (ptr+len) for larger N (more iterations / bigger buffers). cap survives because it's
  the middle word; ptr+len are the clobbered ones.
- Ruled out so far: instruction decoders (str q/ldur q faithful), opt (-O0 no fix), FIX_SP
  (breaks earlier, inconclusive), alias scopes (NO_HOST_SCOPE no fix).
- events-ret (in flight): guarded return-point probe in compact.rs:745 (R_ret_ptr/len/cap/nodes
  at 0x60680). If tier2b is ALREADY (ptr=garbage,len=0) at the return ⇒ CONFIRMED fill clobber
  (the out-param stopgap would NOT fix it — must find the clobbering store). If tier2b is GOOD
  at the return ⇒ it IS the return copy after all (then out-param stopgap works).

## class-B: probe-guard bug + the from_elem connection (events-ret → ret2)
- events-ret R markers all 0 = the guard `p != 0` SKIPPED (tier2b ptr was 0/null at return,
  not just "didn't fire" — bump alloc_count=278 proves the cascade ran). FIXED the guard to
  `p < 4GiB` alone (null ptr now logs) + reached-sentinel 0x600DCAFE@0x60678. ret2 in flight.
- CONNECTION: with_capacity (css/compact_cache.rs:1466) builds tier2b via
  `vec![CompactTextProps::default(); n]` — CompactTextProps::default() is NON-ZERO (sentinel
  fields, e.g. line_height=I16_SENTINEL/0x7ffe per disasm) → `from_elem` (generic SpecFromElem,
  24-byte element), UNLIKE tier1's `vec![0u64; n]` (alloc_zeroed fast path, WORKS). The
  24-byte-element from_elem differs in codegen from the witness's Vec<u64> (8-byte element,
  PASSED). So the suspect narrows to either from_elem's by-value Vec return for a 24B element
  type, or a fill-store clobber — ret2's R probe (corrupt-at-return?) + local-struct store log
  (0x2e800-0x2f000, headers only — buffers are heap) decide.

## class-B FIX ATTEMPT (2026-06-11 ~04:40): from_elem → push-loop
- ret2: probe STILL didn't fire (reached-sentinel 0x60678=0) despite build_compact's body
  running (store ring) — the return-tail probe is unreachable for an unknown reason (likely
  inlining/ordering); abandoned the probe approach.
- ROOT-CAUSE HYPOTHESIS (high confidence): tier1_enums = `vec![0u64; n]` (zeroed → alloc_zeroed
  fast path) WORKS; tier2b_text = `vec![CompactTextProps::default(); n]` where default() is
  NON-ZERO (line_height=I16_SENTINEL) → generic `alloc::vec::from_elem`/SpecFromElem (24-byte
  element, clone-fill, by-value Vec return) → remill MIS-LIFTS → tier2b arrives len=0. Same for
  tier2_dims/tier2_cold (non-zero defaults) — but only tier2b's len=0 CRASHES (index panic in
  collect_font_stacks); the others are silent layout-number bugs (the user-deferred category).
- FIX (web_lift-gated, allowed as web-specific code): `az_filled_vec(elem, n)` in
  css/compact_cache.rs builds via with_capacity + push loop (avoids from_elem; the staged
  witness proved push-built vecs round-trip). Applied to tier2_dims/tier2_cold/tier2b_text.
  Native keeps `vec![elem; n]`. Wired web_lift through css (css/Cargo.toml + core forwards
  azul-css/web_lift). TODO-remove once the transpiler/remill from_elem sret mis-lift is fixed.
- events-fix (in flight): relift web-events.bin (19 nodes) → /tmp/cdp_laidout.js: if it LAYS
  OUT (no unreachable, state set) ⇒ HYPOTHESIS CONFIRMED + class-B unblocked (and likely many
  layout bugs too). Then web-events-cdp.js full 9-kind → S1 100%.

## ❌ from_elem fix FAILED + class-B CONSOLIDATED STATE (2026-06-11 ~04:50)
- az_filled_vec push-loop compiled in (with_capacity 1176B, 0 from_elem calls) but STILL TRAPS
  ⇒ NOT from_elem construction. REVERTED (tree clean; core/layout web_lift feature kept,
  committed). css/core css-web_lift wiring reverted.
- **DECISIVE re-read of the ret2 store ring**: the 0→19 counter loop near the end = the line-743
  `prev_font_hashes = tier2b.iter().map(...).collect()` iterating tier2b 19× ⇒ **tier2b len=19
  was CORRECT at line 743**. So corruption is AFTER 743 = the sret RETURN COPY (and the
  `Some()` move). Resolves the "node-count-independent return copy vs node-count-dependent crash"
  contradiction: hello-world's tier2b ALSO gets corrupted by the same return copy, but its
  garbage len is non-crashing (reads past buffer = wrong fonts, no panic), while web-events
  gets len=0 (→ tier2b[0] index panic). So it IS the build_compact 176-byte sret return copy.
- **The return copy = 11× `str q` (NEON 128-bit) to x24 (sret dest = state+672 via mov x24,x8
  at entry). tier2b ptr@0x50 + len@0x58 ride `str q1,[x24,#0x50]` (corrupt); cap@0x48 rides
  `str q0,[x24,#0x40]` (survives).** BOTH `str q` (0117803d) and `ldur q` lift FAITHFULLY in
  isolation (correct offsets, 2× read/write_memory_64). RULED OUT: instruction decoders, opt
  (-O0), alias scopes (AZ_NO_HOST_SCOPE), FIX_SP (breaks earlier), from_elem construction.
- **REMAINING SUSPECT (for a future focused session)**: remill's memory-model lowering of the
  q-pair HIGH half (the 2nd write_memory_64 → wasm i64.store at dest+8) in the LINKED pipeline,
  OR x24 (sret dest) value threading through the q-copy. Next probes: capture build_compact's
  post-opt + post-lower .opt.ll (scratch watcher) and read the return block's str-q1 lowering;
  OR a targeted store-log windowed to the sret DEST (state+672-derived base) during the return.
  The probe-at-return (compact.rs:745) mysteriously won't fire — abandon source probes for the
  return; use transpiler store-log on the dest instead.
- ⚠ The probe-won't-fire at line 745 despite line 743 running 19× is itself unexplained (CFG
  tail handling between the collect and the bl-return) — worth a look.

## S2 IMPLEMENTED (2026-06-11 ~05:10) — verify pending (events-s2 relift)
- CallbackInfo::new_web (layout/callbacks.rs, web_lift-gated): minimal CallbackInfo with null
  ref_data + the change-sink Arc<Mutex<Vec<CallbackChange>>> + hit node. set_css_property/
  change_text/etc. only use changes + node_id (not ref_data) → null is safe; get_hit_node just
  returns self.hit_dom_node.
- eventloop.rs invoke_node_cb REWRITTEN: builds `Arc::new(Mutex::new(Vec::new()))`, a
  CallbackInfo via new_web (hit = DomNodeId{ROOT, NodeId(node_idx)}), passes &info as info_ptr
  (REPLACES the raw-event-bytes ptr — the linchpin S2 fix). After the cb, s2_drain_changes
  drains via lock()+iter() (NOT take_changes = class-B): ChangeNodeCssProperties/
  OverrideNodeCssProperties → per-property format_css() → SET_INLINE_STYLE TLV;
  ChangeNodeText → SET_TEXT. s2_append_patch accumulates into patch_buf_ptr (4 KiB,
  patch_buf_used tracked). dispatchEvent resets patch_buf_used=0, returns S2 patches when
  present (else the legacy hello-world counter fallback).
- loader_js azApplyPatches case 4: per-decl el.style.setProperty MERGE (was setAttribute clobber).
- BOTH crates cargo-check clean. Test: examples/c/web-setcss-min.c (1 click div, on_click sets
  width:300px on hit node) + scripts/m9_e2e/web-setcss-cdp.js (asserts el.style.width=300px).
- ⚠ LIFT RISK (verify): Arc/Mutex/Vec<CallbackChange> ops + the match + format_css String sret.
  If the CSS doesn't apply, bisect: (a) does the cb even run (counter-style probe)? (b) does
  changes get a push (peek patch_buf_used)? (c) does format_css return a sane string?

## S2 ROOT-CAUSE: cb-side allocation returned 0 (FIX applied 2026-06-11 ~05:30)
- S2 infra WORKS: the cb ran with the real CallbackInfo, reached set_css_property. But it
  TRAPPED there: `vec![property]` → `__rust_alloc(136)` returned 0 → handle_alloc_error →
  unreachable. hello-world's cb never allocated (just increments a counter), so this was hidden.
- ROOT CAUSE: per-cb wasms have "0 mini imports" — each BUNDLES its own BumpAlloc body with its
  OWN `@__az_bump_ptr` (a per-module global / linear-mem copy), which for a cb is uninitialized
  or clobbered by the layout wasm's data-segment mirror → returns 0. mini's works (it's reset +
  not clobbered); the cb's is broken.
- FIX (transpiler_remill.rs): bump pointer moved from the per-module `@__az_bump_ptr` global to a
  FIXED SHARED linear address 0x40020 (262176, inttoptr in every BumpAlloc/Realloc body +
  snapshot/reset helpers). All modules now share ONE bump heap. 0x40020 is in the proven-reliable
  diagnostic band (next to the bump diag at 0x40030), below the mirror (0x110000). Init relies on
  resetBumpHeap(96MiB) (loader line 411, before the first alloc) — the data-segment init is gone.
- events-s2b in flight: web-setcss-min relift. ⚠ REGRESSION RISK: this changes mini's working
  allocator → MUST re-verify hello-world (counter) + web-events-min (S1) after.

## S2 progress: bump fix VERIFIED, node-encoding bug FOUND+FIXED (2026-06-11 ~05:40)
- bump-ptr-shared fix CONFIRMED WORKING: on click the cb allocated (alloc_count 872→887,
  bump@0x40020 advanced to ~103MB). cb-side allocation is UNBLOCKED.
- New trap was MY bug: set_css_property's `.expect("node should not be None")` panicked because
  `hit_dom_node.node.inner` was 0. Cause: invoke_node_cb built the node via
  `from_crate_internal(Some(NodeId::new(node_idx)))` — the Option<NodeId> niche/encoding chain
  mis-lifts to inner=0. FIX: `NodeHierarchyItemId::from_raw(node_idx + 1)` (direct 1-based
  encoding, no Option). events-s2c relifting.
- LESSON: avoid Option<niche-type> construction in lifted glue; use direct/raw constructors.

## S2 trap still in set_css_property after node fix (events-s2c→s2d, 2026-06-11 ~05:50)
- Bump diagnostics post-trap: last_alloc_size=48 → valid ptr (0x62a25e8), alloc_count=887.
  So allocs WORK; the trap is NOT alloc-fail. set_css_property's `.expect` runs BEFORE its
  136-byte vec alloc, so the 48-byte alloc was from the earlier AzCssProperty_width → set_css_
  property's expect (node=0) panics before its own alloc. So node IS still 0 despite from_raw.
- events-s2d (in flight): markers in invoke_node_cb — 0x40090=node_idx, 0x40094=node_idx+1,
  0x40098=0xCB000000|update (written only if the cb RETURNS). Peek post-trap:
  - node_idx sane (e.g. 1) + 0x40098 absent ⇒ mini built the node fine; the cb's getHitNode/
    copy reads hit_dom_node wrong (CallbackInfo layout/copy-size mismatch between mini's build
    and the cb wrapper) → inspect the cb wrapper's info copy.
  - node_idx weird (huge/MAX) ⇒ the hit-test/dispatch fed a bad index → fix routing.

## S2 state consolidated (2026-06-11 ~06:00)
- s2d markers (DEFINITIVE on mini's side): node_idx=1, node+1=2 fed correctly; cb_returned=0
  (cb trapped). So MINI's input is correct; the trap is in the cb's set_css_property.
- The trap is the 16-byte DomNodeId `node` half being 0 by the time set_css_property reads it
  (`.expect("node should not be None")` OR a downstream large-value push — not yet disambiguated
  because the s2e info[16] peek build FAILED VALIDATION). The chain that could drop the node
  half: new_web's struct-literal store of the 16-byte hit_dom_node, getHitNode's 16-byte
  multi-register return, or set_css_property's 16-byte node arg — all CLASS-B-FAMILY (multi-word
  value handling). hello-world never exercised 16-byte struct returns (its cb just increments).
- ⚠ s2e FLUKE: adding one marker read produced an 8-byte mini ("Fatal: error validating input
  (falling back to un-opt'd wasm)"). Either the marker IR or — more likely — the NEW obj cache
  (cache-v2a) served an incompatible object. WATCH: if the hello-world regression ALSO yields an
  8-byte mini, SUSPECT THE OBJ CACHE → disable it (AZ_NO_LIFT_CACHE=1 or guard obj_cache_path
  harder) and re-verify. The obj cache key may not fully capture a change.

## VERIFIED THIS ARC (commit candidates)
- bump-ptr-shared fix (transpiler_remill.rs): cb-side allocation works (alloc_count 872→887 on
  click; bump@0x40020 advances). Needs hello-world regression (in flight) to confirm no
  mini-allocator regression, then COMMIT.
- S2 infra (new_web + eventloop drain + loader case-4 + node from_raw fix): correct + compiles,
  but CSS round-trip BLOCKED by the 16-byte DomNodeId class-B-family gap above. Hold or commit
  with honest "infra; CSS blocked" framing.

## ⚠ OBJ-CACHE POLLUTION confirmed (2026-06-11 ~06:05)
- hello-world regression (cache WARM) → 8-byte mini ("Fatal: error validating input") + cb
  LinkError "memory import must be a WebAssembly.Memory object" (downstream of the stub mini).
  Reverting the marker did NOT fix it ⇒ NOT the marker. It's the cache-v2a OBJECT CACHE serving
  a stale/incompatible object across the bump-fix IR change (likely a bad object cached during
  the s2e validation-failed build, OR a key that missed the bump-body change). s2b/c/d worked
  because their cache was freshly populated post-bump-fix; a later build polluted it.
- ACTION TAKEN: cleared $TMPDIR/az-lift-cache, cold re-relift hello-world (bsn7otixv).
- PERMANENT FIX (after confirming clear works): bump LIFT_CACHE_VERSION (invalidates all
  obj-cache entries) AND make obj_cache_path NEVER cache an object whose build later fails
  validation/link (cache only fully-validated outputs), or fold a hash of emit_helper_ir's
  bump/global section into the key. Until fixed, run debug relifts with AZ_NO_LIFT_CACHE=1.

## OBJ-CACHE POLLUTION CONFIRMED + FIXED; hello-world DISPLAY regression (2026-06-11 ~06:15)
- Cleared cache → mini valid (27MB un-opt'd; the "error validating input → un-opt fallback" is
  PRE-EXISTING/harmless, 2× in the very first relift too). ⇒ obj-cache pollution was the 8-byte
  mini. PERMANENT FIX: bumped LIFT_CACHE_VERSION 2→3 (invalidates all stale obj-cache entries).
  TODO still: harden obj_cache_path to never cache validation-failed builds (defensive).
- hello-world REGRESSION (S2 infra): the cb RUNS and increments the model (peeked 5→6, no
  exceptions) but the DISPLAY stays 5 — dispatchEvent emits NO patch (out_len=0 on the kind=0
  click). So the cb works; the counter-fallback patch path doesn't fire post-S2. hwreg3 markers
  (0x400A0 update / A4 model_ptr / A8 display_text_node_idx / AC patch_buf_used) will say why:
  update=0 ⇒ RefreshDom return lost across the bigger invoke_node_cb frame (Arc/info/s2_drain);
  display=MAX ⇒ my new patch_buf_used struct field shifted the layout; else ⇒ buildCounterPatch
  alloc.

## CONSOLIDATING to green (2026-06-11 ~06:25): S2 infra REVERTED
- hwreg3 markers: update=1 (RefreshDom return fine), but s.model_ptr=0 + s.display_text_node_idx=0
  at dispatch (both set by the loader's setModelPtr/setDisplayNode, which ARE reached — the cb
  works so we pass the !azRefAnyPtr guard). The cb increments the model (5→6) but the
  counter-fallback patch never fires (model_ptr=0). model_ptr/display are EARLY struct fields
  (offsets unchanged by my end-additions), setter+reader share one build — yet read 0. Root
  cause not quickly found (possible EventloopState default-repr re-pack interaction, or a
  later write zeroing them).
- DECISION (session very long, S2 blocked by 16-byte DomNodeId anyway): reverted the UNCOMMITTED
  S2 infra (git checkout eventloop.rs + loader_js.rs + callbacks.rs → committed S1 state). KEPT:
  bump-ptr-shared fix + LIFT_CACHE_VERSION=3 (transpiler_remill.rs, uncommitted) + web-setcss
  test files. hwreg4 = S1+bump relift of hello-world to test this baseline.

## ✅ GREEN BASELINE RESTORED + bump fix COMMITTED (2026-06-11 ~06:35)
- hwreg4: hello-world counter 5→6 DISPLAYS with S1 + bump fix (S2 reverted), mini valid (26MB).
  ⇒ bump fix is CLEAN; the display regression was entirely the S2 infra. COMMITTED the bump
  fix + LIFT_CACHE_VERSION=3 (commit 2d535469e).
- s1reg (in flight): web-events-min relift to confirm S1 routing still green with the bump fix.

## S2 RE-APPROACH (focused session) — two precise blockers, both likely fixable
The S2 infra (real CallbackInfo + CallbackChange→TLV drain) was reverted from the working tree
but is fully specified in this file's S2 recipe (and recoverable from git reflog of eventloop.rs
before commit 2d535469e). When re-approaching:
1. **16-byte DomNodeId class-B-family gap**: set_css_property's `.expect("node should not be
   None")` panics because hit_dom_node.node is 0 in the cb. Mini FED node_idx=1/+1=2 correctly
   (s2d markers), so the loss is in new_web's 16-byte hit_dom_node store OR getHitNode's 16-byte
   multi-register return OR set_css_property's 16-byte node arg. FIX ANGLES: (a) a web-specific
   getHitNode/setCssProperty path that passes node as a single u64; (b) field-store the node
   half explicitly; (c) standalone-lift getHitNode + new_web, diff the 16-byte struct
   store/return, fix in remill (the multi-word value handling — same family as class-B).
2. **model_ptr=0 dispatch regression**: with the S2 dispatchEvent/EventloopState changes, the
   counter-fallback's s.model_ptr/display_text_node_idx read 0 (set by the loader, but read 0 at
   dispatch). Suspect: adding `patch_buf_used` IN THE MIDDLE of EventloopState (between
   focused_node_idx and viewport_w) perturbed the default-repr re-pack such that some lifted
   accessor and the reader disagree. FIX: add new EventloopState fields ONLY at the very END
   (or make the struct repr(C) to freeze the layout), and re-test hello-world's display
   alongside web-setcss.
3. cb-side allocation is ALREADY FIXED (committed bump fix) — that prerequisite is done, so the
   S2 re-approach starts from a cb that CAN allocate.

## ✅ BASELINE FULLY GREEN + COMMITTED (2026-06-11 ~06:40)
- s1reg: web-events-min CDP green (click 0→1, keydown 0→1, resize 0→2) with the bump fix.
  hello-world green (5→6 display). Committed this session: S0 (preflight+cache), remill NZCV
  decoder, S1 (input routing), generic hydration, obj-cache, and the SHARED BUMP HEAP + cache-v3
  (2d535469e). Tree clean (only untracked web-setcss S2 scaffolding + this superplan).

## 🎯 CLASS-B = THE GATE for S2-S7. MINIMAL REPRODUCER FOUND: AzCallbackInfo_getHitNode
- getHitNode (native 0x52eb0, 4 instructions): `ldr x8,[x0,#8]; ldr x1,[x0,#0x10]; mov x0,x8; ret`
  — returns the 16-byte DomNodeId in x0:x1. STANDALONE LIFT IS FAITHFUL (reads info+16 via
  read_memory_64 → X1, returns both halves). But in the cb PIPELINE the node (x1) half is lost
  → set_css_property sees node=0 → `.expect` panics. SAME class as build_compact's 176-byte
  q-pair return copy: faithful standalone, broken in pipeline.
- This is the SMALLEST class-B witness yet (the prior handoff's 24-byte Vec witness PASSED
  because it was built via with_capacity+push, NOT a multi-register struct return). getHitNode
  = a multi-word VALUE returned in registers / read from memory → the 2nd register/word drops.
- RULED OUT (this session, exhaustively): instruction decoders (str q / ldur q / getHitNode all
  faithful standalone), opt level (-O0 no fix), alias scopes (AZ_NO_HOST_SCOPE no fix), FIX_SP
  (breaks earlier), from_elem construction (push-loop no fix). The memory intrinsic bodies
  (transpiler_remill.rs:6953+ read/write_memory_*) are correct (plain volatile load/store).
- ⇒ The bug is a PIPELINE transform over the linked module — opt mis-handling consecutive
  volatile memory-64 ops feeding a multi-register return/struct, OR the wasm multi-value/struct
  ABI lowering dropping the high register. NEXT-SESSION EXPERIMENT (bounded, high-value):
  lift getHitNode standalone, then run it through the FULL pipeline (llvm-link helper IR + opt
  + llc) in isolation and diff — find where x1 (node) is dropped. Capture the .opt.ll. If opt
  drops it, find the offending pass (try opt -O1/-O0 on JUST getHitNode via AZ_LOWOPT_FNS; if
  -O0 keeps x1, it's an opt transform → bisect passes; if -O0 also drops it, it's the link /
  ABI lowering / llc). Then fix in azul_remill.cpp's pipeline or the helper IR.

## CLASS-B BARRIER HYPOTHESIS — testing (2026-06-11 ~06:50)
- strip_noalias_from_sub_args IS applied (strips noalias from sub_ defines). The g188/g189
  comment in tag_state_accesses records the prior session's key finding: the class-B-family bug
  is a BARRIER-DEFEATABLE optimizer mis-transform (load-forward / DSE / reorder of mem ops); a
  volatile barrier makes it vanish, but a lean barrier pass was NEVER implemented.
- getHitNode's State-register stores (`store i64 …, ptr %X1`) are NON-volatile → opt can
  forward/DCE them across calls (the caller reads a stale X1/node). The guest MEMORY ops are
  volatile, but the STATE REGISTER writes are not.
- DECISIVE CHEAP TEST (no code change): AZ_FUEL=ALL injects per-terminator volatile barriers
  (traps only after 200M iters, far above layout loops). Relifting web-events.bin (the 19-node
  class-B trap in collect_font_stacks) with AZ_FUEL=ALL — if it LAYS OUT, barriers fix class-B →
  implement a lean barrier (a `call void asm sideeffect "", "~{memory}"()` after sub_ calls, or
  make State stores volatile) and re-test. (events-fuel relift in flight; fuel ALL is slow.)
- ⚠ CORRECTION: the S2 infra was UNCOMMITTED and discarded by `git checkout` — it is NOT in
  reflog. Re-implement from the S2 recipe in this file when class-B is fixed.

## CLASS-B: barrier DISPROVEN + standalone repro FAILED (2026-06-11 ~07:05) — context-dependent
- AZ_FUEL=ALL relift of web-events.bin: STILL TRAPS in collect_font_stacks (same chain
  sub_aab2b4←aae018←b5b738). ⇒ AZ_FUEL's volatile barriers do NOT fix class-B (confirms g189's
  caveat that the "barrier vanishes it" observation was confounded). Barrier hypothesis DEAD.
- Standalone pipeline test (/tmp/ghn_test.ll: getHitNode sub_52eb0 + synthetic caller that
  stores info→X0, calls it, loads X1/node) through `opt -O2` (homebrew LLVM-21): the X1 load
  AFTER the call is PRESERVED + correct (sub_52eb0 not inlined, tail call, no forward/DCE). Even
  with the `initializes` attr (LLVM-19+, the handoff's red herring), opt does NOT drop X1.
  ⇒ class-B does NOT reproduce on an isolated caller+callee. It is CONTEXT-DEPENDENT: needs the
  real failing cb + the FULL linked module (hundreds of fns) + the real LLVM-17 in-proc pipeline.
- ⇒ EXHAUSTIVELY RULED OUT: instruction decoders, opt level (-O0), alias scopes (NO_HOST_SCOPE),
  FIX_SP, from_elem construction, AZ_FUEL barriers, isolated-opt (LLVM-21). The bug is a
  full-module / LLVM-17-specific optimization mis-transform.

## ⛔ CLASS-B IS HARD-BLOCKED via every approach executable in this loop. It is the prior
## session's known-hard bug (25+ iters there). It gates S2-S6.
ONLY remaining diagnostic (deliberate, deep — for a focused session / user direction):
- Add a post-opt IR DUMP to dll/src/web/cpp/azul_remill.cpp (the in-proc LLVM-17 pipeline) for a
  named function (build_compact_cache_with_inheritance_debug or the cb's set_css_property) — dump
  the module IR right after `buildPerModuleDefaultPipeline(Oz)` runs over the LINKED module.
  Then inspect whether LLVM-17 drops the tier2b q-store / the X1-node store in the FULL-module
  context. That pinpoints opt-vs-llc and the offending transform → fix in azul_remill.cpp
  (e.g. pin the pipeline, disable the offending pass, or add a real barrier the right way).
- ALTERNATIVELY (if not chasing the deep fix): accept the green baseline as the deliverable for
  cbs that don't return multi-word structs, and ask the user whether to invest in the class-B
  LLVM-17 dive or ship S0/S1 + bump as-is.

## GREEN BASELINE (shippable checkpoint, all committed)
S0 (preflight+cache) · remill NZCV decoder (70050e0) · S1 input routing (3438d602e) · generic
hydration (43b2a455f) · obj-cache+store-log (4091af06d) · shared bump heap + cache-v3 (2d535469e)
· test apps (bfab10060) · plan (081c85f0c). hello-world (5→6 display) + web-events-min (S1
click/keydown/resize) CDP-green. Tree clean (untracked: web-setcss S2 scaffolding).

## IR DUMP partial (2026-06-11 ~07:30): captured the DISPATCHER, not build_compact's body
- AZ_DUMP_OPT_IR (text-search) in azul_remill.cpp compile_inner produced /tmp/az_optdump_0.ll
  (868KB) = the MERGED MINI DISPATCHER (one giant fn = switch over all sub addrs, each case
  `tail call @sub_<x>`; it only DECLAREs sub_e69d64/build_compact). 0 volatile stores there
  (expected — it's pure tail calls). build_compact's actual body did NOT dump.
- ROOT of the dump miss: build_compact's per-fn object does NOT go through compile_inner (the
  only buildPerModuleDefaultPipeline call, line ~754). There is ANOTHER compile path for per-fn
  objects (or build_compact is in a batch the dump filter missed). NEXT: find it — grep
  azul_remill.cpp for every opt/codegen entry, OR add a one-line stderr `fprintf(stderr,
  "[dump] compile_inner module fns=%zu first=%s\n", …)` to see which modules reach compile_inner
  and what build_compact's per-fn module is named, then dump THAT.
- ⚠ The dump scaffolding is UNCOMMITTED in azul_remill.cpp (revert before any commit). The
  dylib on disk has it (next wake can re-relift WITHOUT rebuild if the dump code is reused).

## ⏱ STOP CONDITION (this loop has spent many wakes on class-B — the prior session's known-hard
## bug — without cracking it; the green S0/S1/bump baseline IS shippable):
If the next 1-2 wakes do NOT capture build_compact's body + find an actionable opt/llc fix:
CONSOLIDATE — revert the dump scaffolding, write a final "class-B = deep remill/LLVM-17 gate,
green baseline shipped" status, and surface to the user as a decision point (invest in the deep
class-B fix, or ship S0/S1/bump and accept per-API verification on non-multi-word-struct cbs).
The minimal reproducer (getHitNode) + the exhaustive ruled-out list are the handoff artifacts.

## (historical) IR DUMP IN FLIGHT (2026-06-11 ~07:10): azul_remill.cpp post-opt dump added
- Added AZ_DUMP_OPT_IR=<fn-substr> to azul_remill.cpp (after buildPerModuleDefaultPipeline.run):
  dumps the REAL LLVM-17 post-opt module IR for the bundle containing a matching fn → /tmp/
  az_optdump_N.ll. events-dump relift (bnm1i341f) running with
  AZ_DUMP_OPT_IR=build_compact_cache_with_inheritance_debug.
- ON WAKE: grep /tmp/az_optdump_*.ll for the build_compact return-copy stores to the sret dest
  (tier2b ptr@+0x50 / len@+0x58 — the q1 store). If they're DROPPED/forwarded in the LLVM-17
  post-opt IR → opt is the culprit (find/disable the pass, or pin the pipeline). If PRESENT in
  post-opt IR but the runtime still corrupts → llc (the wasm codegen / multi-value lowering)
  drops it. That's the opt-vs-llc split. Then fix in azul_remill.cpp.

## (historical) Next action
1. (deep, deliberate) azul_remill.cpp post-opt IR dump of build_compact in the full LINKED module
   → find the LLVM-17 transform that drops the multi-word store → fix → relift web-events →
   cdp_laidout green ⇒ class-B fixed ⇒ unblocks S2 (re-implement infra from recipe + fix the
   EventloopState re-pack) and all large apps.
2. If class-B proves intractable again: this is a user-decision point — keep grinding the LLVM-17
   dive, or accept the green S0/S1/bump baseline. The cron stays until the user decides.

## (historical) next wake — class-B via the getHitNode minimal reproducer
1. Build the standalone getHitNode → full-pipeline harness (or AZ_LOG_STORES / AZ_LOWOPT_FNS on a
   relift of web-setcss-min with the S2 infra restored) to pin WHERE x1/node is dropped. Fix it.
2. When class-B is fixed: restore the S2 infra (recipe above; recoverable from git reflog of
   eventloop.rs/callbacks.rs/loader_js.rs before 2d535469e — they were reverted, not deleted),
   ALSO fix the model_ptr=0 regression (add EventloopState fields at the END / make it repr(C)),
   relift web-setcss-min → width→300px, regression hello-world + web-events-min → COMMIT S2 +
   the web-setcss test scaffolding. Then S3 (timers), S4 (images), etc.
3. If class-B stays intractable: it's the prior session's known-hard bug; the green baseline
   (S0/S1/bump) is a solid, shippable checkpoint. Consider asking the user whether to keep
   grinding class-B or accept per-API verification only on cbs that don't return multi-word
   structs.

## (historical) hwreg4 relift up
1. /tmp/cdp_drive.js → counter 5→6 DISPLAY:
   - GREEN ⇒ bump fix is clean, the regression was the S2 infra → COMMIT bump fix + cache-v3
     (semantic, transpiler). S1 + bump baseline is solid. Then re-approach S2 in a FOCUSED
     session: the 16-byte DomNodeId class-B-family gap + the model_ptr=0 dispatch regression
     are the two things to solve (likely both fixable; the S2 infra diff is recoverable from
     git reflog / this superplan's S2 recipe).
   - RED (still 5→5) ⇒ the regression is in COMMITTED S1 or the bump fix. Bisect: revert the
     bump fix too (git checkout transpiler_remill.rs) → relift; if green, bump fix is the
     regression (it corrupts EventloopState via allocation/init — investigate the shared-bump
     init/overlap); if still red, committed S1 broke hello-world's display (a real committed
     regression to fix — likely the dispatchEvent rewrite's counter-fallback path).
2. End at a GREEN, committed baseline. Don't leave hello-world broken.

## (historical) hwreg3 relift up
1. Peek 0x400A0/A4/A8/AC → diagnose per above.
   - update=0: restructure invoke_node_cb so the cb's Update is preserved (capture before
     s2_drain into a volatile/explicit slot; or call s2_drain via a helper that can't clobber).
   - display=MAX or model=0: struct-field shift — verify EventloopState layout / setters.
2. Fix → hello-world green (5→6 display) → COMMIT bump fix + cache-v3 + the dispatch fix. THEN
   decide S2 infra: it's blocked by the 16-byte DomNodeId gap AND this regression — if both fixed,
   keep; else REVERT S2 infra to a green baseline and defer S2-CSS to a focused session.
   ⚠ The session is very long with S2 regressions — prioritize returning to a GREEN, committable
   baseline (bump fix + cache fix + hello-world + S1 all working) over finishing S2-CSS.

## (historical) cold hello-world relift up
1. mini NOT 8 bytes + counter 5→6 ⇒ obj-cache pollution confirmed → bump LIFT_CACHE_VERSION +
   harden obj cache → COMMIT bump fix. If STILL 8-byte mini ⇒ the bump fix itself emits invalid
   wasm (revert bump fix, route cb alloc via import instead).
2. Then S2 16-byte DomNodeId gap (peek info+16 with AZ_NO_LIFT_CACHE=1 to avoid pollution):
   node=2 → getHitNode 16-byte return drops it; node=0 → new_web store drops it.

## (historical) events-s2d relift up
1. node /tmp/cdp_clicktrap.js (confirm still trapping) + peek 0x40090/94/98 (AzStartup_peekU32)
   → diagnose per above → fix → relift → web-setcss-cdp.js green → regression → COMMIT.

## (historical) events-s2c relift up
1. node scripts/m9_e2e/web-setcss-cdp.js → width→300px ⇒ S2 set_css_property round-trips →
   then REGRESSION: hello-world (counter, /tmp/cdp_drive.js) + web-events-min (S1) → all green ⇒
   COMMIT: (a) bump-ptr-shared fix [transpiler], (b) S2 infra [new_web + eventloop drain +
   loader case-4], (c) node-encoding fix, (d) test app/harness. Semantic commits.
2. If web-setcss STILL traps: get the new trap (cdp_clicktrap.js). If it's downstream of
   set_css_property (the push of large CssProperty/CallbackChange) → class-B large-value →
   document, commit the bump fix alone, defer S2-CSS. If hello-world regressed → revert bump fix.

## (historical) events-s2b relift up
1. node scripts/m9_e2e/web-setcss-cdp.js → width→300px ⇒ cb-alloc fix + S2 both work → then
   REGRESSION: relift examples/c/hello-world.bin + /tmp/cdp_drive.js (counter 5→6) AND
   web-events-min (S1). All green ⇒ COMMIT (S2 infra + bump-ptr-shared fix + setcss test).
2. If web-setcss still traps in alloc → the fixed-addr 0x40020 is clobbered or reset-timing is
   off; peek 0x40020 post-trap (AzStartup_peekU32) to see the bump value. If hello-world
   REGRESSES → the shared-bump change broke mini; revert the bump change, keep S2 infra, and
   route cb alloc differently (make cb IMPORT __rust_alloc from mini instead of bundling).

## (historical) events-s2 relift up
1. node scripts/m9_e2e/web-setcss-cdp.js → width→300px ⇒ S2 set_css_property round-trips →
   COMMIT S2 (new_web + eventloop drain + loader case-4 + test app/harness). Also re-run
   web-events-min-cdp.js as an S1 regression (the dispatch path changed).
2. If CSS doesn't apply → bisect per the lift-risk note; likely the CallbackChange Vec/match or
   format_css. Fall back: single-variant drain, or peek patch_buf_used to localize.
3. Then S3 (timers) on a minimal app.

## (historical) PIVOT to S2
S1 is committed + verified on a minimal app. The same minimal-app strategy verifies S2-S7 — so
class-B (the known-hard prior-session sret bug, now thoroughly localized above) does NOT block
the deliverable. Proceed:
1. **S2 — callback modifies CSS, wasm cascades, TLV sets inline style.** Concrete recipe
   (dll/src/web/eventloop.rs `invoke_node_cb` / `dispatchEvent`):
   a. Build the changes sink in lifted Rust: `let changes: Arc<Mutex<Vec<CallbackChange>>> =
      Arc::new(Mutex::new(Vec::new()));` (Arc=atomic refcount lifts; Mutex=os_unfair_lock=Leaf
      no-op = correct single-thread).
   b. Build CallbackInfo via `CallbackInfo::new(ref_data, &changes, hit_dom_node, None, None)`
      (approach B — lets the constructor handle the repr(C) layout; no manual offsets). ref_data:
      set_css_property does NOT deref it (only uses the passed node_id + self.changes), so pass a
      FAKE &CallbackInfoRefData = `unsafe { &*(some_zeroed_alloc as *const CallbackInfoRefData) }`
      — the ctor only stores the pointer. hit_dom_node = DomNodeId{ dom: DomId{inner:0}, node:
      NodeId/AzNodeId from node_idx }. Pass `&info as *const CallbackInfo as u32` as the info_ptr
      to __az_call_indirect (REPLACES event_bytes_ptr — THE linchpin fix; today every
      AzCallbackInfo_* reads event bytes as CallbackInfo).
   c. After the cb returns, DRAIN in place (NOT take_changes() — returns Vec by value = class-B):
      `if let Ok(cs) = changes.lock() { for c in cs.iter() { match c { ... } } }`. Translate:
      ChangeNodeCssProperties{node_id, properties} / OverrideNodeCssProperties → for each
      `properties.iter()`: `p.format_css()` (→ "key: value;", 24B String sret = small, the
      PASSING witness class) → emit PATCH_KIND_SET_INLINE_STYLE TLV (node_id, css-decl bytes)
      via AzStartup_buildPatch into a growable patch buf; ChangeNodeText{node_id,text}→SET_TEXT;
      SetFocusTarget→FOCUS; ScrollTo→SCROLL_TO. Accumulate multiple patches (loop appends TLVs).
   d. JS dll/src/web/loader_js.rs azApplyPatches case 4 (SET_INLINE_STYLE): change from
      `el.setAttribute('style', css)` (clobbers server inline styles) to per-declaration parse +
      `el.style.setProperty(name, value)` MERGE (split payload on ';', each "k: v").
   e. Test: examples/c/web-setcss-min.c — 1 click div, on_click calls
      `AzCallbackInfo_setCssProperty(info, hit_node, background-color: red)` + width; CDP click →
      assert el.style.backgroundColor changed. ~4 nodes (under class-B).
   ⚠ LIFT RISK to watch: Arc/Mutex/Vec<CallbackChange> ops + the match — if any mis-lift, bisect
   with a single-variant drain first. CssPropertyVec.iter() + format_css String return are the
   main sret concerns (small, expected OK).
2. Then timers (S3), images/fonts (S4), etc. — all on minimal apps until class-B is fixed.

## (historical) ret2 relift up
0. node /tmp/cdp_patch_probe.js → R markers: reached-sentinel 0x600DCAFE@0x60678 present?
   - present + R_ret_len=0/ptr small ⇒ corrupt AT return → read the local-struct store ring
     for the write that zeroes tier2b's header → trace to from_elem return or a fill store →
     fix in transpiler/remill. (out-param stopgap will NOT help a from_elem/fill bug.)
   - present + R_ret_len=19 ⇒ clean at return → it IS build_compact's own sret return-copy →
     out-param stopgap unblocks.
   - sentinel ABSENT ⇒ probe still not reached → build_compact runs elsewhere / a different
     compact path; re-locate (grep server log for which sub builds the cache hydrate vs layout).

## (historical) events-ret relift up
1. node /tmp/cdp_patch_probe.js → R_ret_* (0x60680):
   - len=0 at return ⇒ fill clobber confirmed → instrument build_compact's LOCAL frame
     (AZ_LOG_STORES windowed to its SP region) to catch the store that zeroes tier2b's local
     len → it's a mis-lifted element-index store address (likely a multiply/add or a NEON
     element store) → fix in remill/transpiler. NO stopgap shortcut.
   - len=19 at return ⇒ return-copy after all → apply out-param stopgap to unblock S2+.
2. Fix/stopgap → web-events lays out → web-events-cdp.js full 9-kind → S1 100% → S2.

## (historical) AZ_NO_HOST_SCOPE plan
1. AZ_NO_HOST_SCOPE=1 relift (env-only, warm-ish) → markers: if tier2b GREEN, the host/guest
   alias-scope tagging on the q-pair sret copy is wrong → fix the tagger (don't mark the
   sret-dest stores noalias vs the source field loads). If still bad → local-frame instrument:
   AZ_LOG_STORES=build_compact… windowed to build_compact's SP region, id-floored to the tail,
   to see if tier2b's LOCAL slot is already (ptr=garbage,len=0) before the return copy reads it
   (distinguishes memory-model-lowering bug vs body-store-clobbers-local).
2. If proper fix lands → relift web-events.bin (19 nodes) → web-events-cdp.js full 9-kind green
   → S1 100%. Else apply the out-param STOPGAP, relift, verify web-events, then proceed to S2.
3. S2 (real CallbackInfo + CSS-out via TLV) once any non-trivial app lays out.

## (historical) earlier Next action
1. web-events-min.bin relift up → if it lays out (under threshold): node
   scripts/m9_e2e/web-events-min-cdp.js → click + keydown-broadcast + resize-broadcast all
   increment ⇒ S1 VERIFIED → COMMIT the S1 batch (remill NZCV already committed; this commits
   eventloop.rs/loader_js.rs/html_render.rs S1 + hydration size/bytes + RefAny::get_data_len +
   core web_lift feature + transpiler obj-cache + AZ_LSWIN_LO/ID + triage script + the test
   apps/scripts). REVERT the getters.rs + styled_dom.rs probes + az_sret witness first.
   - If min ALSO traps (threshold ≤6 nodes): class-B blocks all non-trivial apps → must fix it
     before S1 can be e2e-verified. Apply the sanctioned out-param workaround to
     build_compact_cache_with_inheritance (web_lift-gated, documented TODO-remove) as a STOPGAP
     to unblock, then deep-fix.
2. class-B: AZ_NO_HOST_SCOPE test → local-frame instrument → permanent transpiler/remill fix.
3. Resume S2 (real CallbackInfo + CSS-out).

## (historical) events16 = build_compact + with_capacity forced -O0 via AZ_LOWOPT_FNS
1. node /tmp/cdp_patch_probe.js → A_afterbuild_*:
   - GREEN at O0 (ptr valid, len=19): CONFIRMED optimizer transform. Narrow: try AZ_NO_HOST_SCOPE=1
     (alias-scope off) at normal opt — if that also fixes it, the bug is the host/guest alias
     tagging on the sret copy → fix the tagging (don't mark sret-dest stores noalias vs the
     struct's field loads). Else it's a generic opt (DSE/forward) → add a volatile barrier on
     the sret return-copy stores in the transpiler.
   - STILL BAD at O0: not opt — it's the lift/helper/fixsp. Bisect AZ_NO_FIX_SP=1, then the
     memcpy/memmove helper body, then remill sret semantics.
2. Permanent fix in transpiler/remill (NOT a per-fn -O0 hack, though that's a valid STOPGAP to
   unblock S1 if the real fix is deep). Relift → markers green → CDP suite → hello-world
   regression → COMMIT batch → S2.
3. Parallel cheap track still open: web-Ntext.c binary-search for minimal trapping N.

## (historical) earlier events14 note
1. node /tmp/cdp_patch_probe.js → A_afterbuild_* (0x60660, post-build pre-move) + B_aftermove_*
   (0x60670, post Some-move):
   - A bad (ptr=3/len=0): lifted build_compact RETURN garbles tier2b — sret/return mis-lift of
     the 4th Vec field; instrument the return q-copy by heap window; suspect i128/q-pair chain
     or FIX_SP over the inlined return.
   - A good + B bad: the Some()-move memcpy drops tier2b — chase LibcMemcpy/llvm.memmove body/len.
   - both good: later aliasing write — window heap 0x6046xxx across the layout pass.
2. Fix in transpiler/remill → relift → markers green (tier2b len=19, no trap) → full CDP suite
   → hello-world regression → COMMIT batch (S1 + hydration + obj-cache + AZ_LSWIN_LO/ID +
   core web_lift + the fix + stack-slot fix) → revert probes/witness → S2.

### CDP harness relaunch (cron/next-session)
If `curl -s http://127.0.0.1:9222/json/version` is down, relaunch headless Chrome:
`"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --remote-debugging-port=9222 --user-data-dir=/tmp/cdp_profile --no-first-run --disable-gpu about:blank &`
Then: relift (`bash scripts/web_relift.sh examples/c/hello-world.bin /tmp/server.log`, ~6min),
drive (`node /tmp/cdp_drive.js` — counter 5→6 = green), peek (`node /tmp/cdp_peek_multi.js http://127.0.0.1:8800/ 0xADDR1,0xADDR2`).
If /tmp/cdp_*.js are missing, recreate from the inline heredocs in the session history (peek = AzStartup_peekU32 wrapper; drive = navigate + console dump).
