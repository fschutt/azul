# Shaped-text refactoring plan — the Gecko compact-record model for `text3`

Written 2026-08-07 against azul `7a10c661f` (working tree has three
uncommitted files — see §0.4). **This is a PLAN. Nothing in it is
implemented and nothing here should be implemented until §3 is done.**

Target: `azul-layout`'s `text3` retains **51.4 MB of shaped text on a
960-line realistic markdown — 79% of everything the document retains**
(`scripts/RSS_MAP_2026_08_07.md` §7t, §9). This plan replaces the retained
representation with a compact per-cluster record plus a side table, the
model Gecko uses to hold the same information in ~6 B/char.

---

## 0. How to read this, and what is evidence

### 0.1 Evidence legend

Every factual claim below carries one of three markers. Where none is
given, the claim is **MEASURED**.

| tag | means |
|---|---|
| **MEASURED** | a number from `scripts/RSS_MAP_2026_08_07.md` (heaptrack / smaps / `AZ_PROFILE=memory`), or a fact read directly out of the source at the cited `file:line` |
| **INFERRED** | arithmetic or reasoning on measured inputs; the derivation is always shown |
| **UNKNOWN** | not established; listed so it is not mistaken for either of the above |

### 0.2 Source documents, in reading order

1. `scripts/RSS_MAP_2026_08_07.md` **§0 first** — it names which of ~15
   self-corrections is current for each quantity. Then §9 (verdict
   table), §10 + §12 (per-cluster cost), §7k + §20 (the duplication),
   §17 (re-measuring).
2. `scripts/LAYOUT_MEMORY_ALLOCATION_RESEARCH.md` — external-engine
   evidence. §4.2 Gecko, §4.4 Blink/NGFragmentItem, §4.5 WebRender
   interning, §4.6 the Stylo Arc-sharing failure, §8 the prior P0–P6
   plan this document supersedes, §9 the "could not verify" ledger.
3. `scripts/rss-baseline.sh` — the committed re-measurement procedure and
   its baseline (in the file header).

### 0.3 The three numbers this plan is built on

| quantity | value | source |
|---|---:|---|
| shaped text, realistic 960-line doc | **51.4 MB** (79% of the document's 65.0 MB retained) | §0, §7t, §19 |
| per-cluster retained cost | **≤853 B** engine accounting / **1 460 B** allocator bytes | §12, §19 |
| the comparable external figure | Gecko **~6 B/char** retained (4 B `CompressedGlyph` + 2 B UTF-16), with a `DetailedGlyph` side table | §10, RESEARCH §4.2 |

Ratio: **~140–240x**. The HarfBuzz "24 B/glyph" comparison that appears
in older notes is a **category error** and must not be used — that is a
transient shaping buffer, ours is retained layout state (§10).

The two per-cluster figures are computed on 32 937 clusters:
`≤28.09 MB / 32 937 = ≤853 B` (engine's own two buckets, after the §12
over-count correction) and `48.09 MB / 32 937 = 1 460 B` (heaptrack
`text3::cache` less `FontManager`, §19). They measure different things and
both are quoted deliberately: 853 is what the data structures logically
hold, 1 460 is what the allocator hands the process.

### 0.4 The instrument was fixed AFTER the report was written

`e83305bd8 fix(layout): AZ_PROFILE=memory over-counted glyphs and missed
four things` landed while this plan was being written. It applies the §12
fix — the `SmallVec` inline-slot double-count — to `text3/cache.rs`:
`glyph_spill_bytes` at `layout/src/text3/cache.rs:6242-6244`, used at
`:6252` and `:6286`, with the negative control at
`layout/tests/struct_sizes.rs:180`. It also adds `ARC_HEADER` /
`hashmap_bytes` map-overhead accounting (`cache.rs:6135-6148`,
`:6320-6329`) and the `CombinedBlock` arm the old walk skipped.

**Consequences for anyone reading numbers:**

1. `RSS_MAP_2026_08_07.md` §0 caveat 1 — *"`AZ_PROFILE=memory`
   over-reports `TextShapingCache` by ≥3.4 MB … if you run the engine's
   report yourself, subtract it"* — **is now OBSOLETE. Do not subtract.**
2. The engine now prints `B/cluster` directly (`window.rs:3211-3213`), so
   the ≤853 B figure that §12 derived on paper is measurable in one run.
   **Take a fresh `AZ_PROFILE=memory` reading before step 1 and use it,
   not the paper figure.**
3. The engine also now prints a units note and a `NOT COVERED` list
   (`window.rs:3226`, `:3229`) — worth reading once, since it names the
   app-side `Solver3LayoutCache` that the report spent five sections
   chasing.

The tree is live and other work is in flight (`git status` shows
`layout/src/probe.rs` and `layout/src/window.rs` modified at time of
writing). Check `git log --oneline -5` and `git status` before trusting
any line number in §1.

### 0.5 Three options are SETTLED — do not re-open

| option | verdict | evidence |
|---|---|---|
| custom allocator / arena | **NO** | USER ruling; Servo built one, benchmarked it (arena 216 ns/iter vs plain `Box` 19) and abandoned it; `nsPresArena` spent 3.5 years with half its memory as size-class slop (RESEARCH §3, §4.3) |
| lazy / trimmed font loading | **NO, for this problem** | 8.8 MB resident of 18.8 MB mapped; mmap already works (§9) |
| GPU rendering to save memory | **NO** | measured **+67 MB** — removes ~9.3 MB of framebuffer, adds ~65 MB of driver residency (§7n) |

Also settled, and not a defect: **miniword's app-side pagination layout is
intentionally separate from the engine's.** It costs 21.2 MB (§7p) and
~10.1 MB of that is shaped text going through the same `text3` code, so it
benefits from this refactor for free. Do not propose merging it.

---

## 1. What the code actually does today

This section is read out of the source, not out of the report. Every line
number is current as of `7a10c661f` + the §0.4 working-tree changes.

### 1.1 The retained types and their sizes

Pinned in `layout/tests/struct_sizes.rs:44-94`:

| type | bytes | definition |
|---|---:|---|
| `PositionedItem` | 200 | `layout/src/text3/cache.rs:4796` |
| `ShapedItem` | 184 | `layout/src/text3/cache.rs:4587` |
| `ShapedCluster` | 176 | `layout/src/text3/cache.rs:4661` |
| `ShapedGlyph` | 96 | `layout/src/text3/cache.rs:4698` |
| `LayoutFontMetrics` | 32 | `layout/src/text3/cache.rs:2203` |
| `UnifiedLayout` | 64 | `layout/src/text3/cache.rs:4803` |
| `InlineItemMetrics` | 32 | `layout/src/solver3/layout_tree.rs:180` (INFERRED: not pinned; the field list is `Option<NodeId>` 16 + 3×f32 + bool + u32) |

`ShapedCluster` contains a `SmallVec<[ShapedGlyph; 1]>`, whose inline arm
is `max(size_of::<ShapedGlyph>(), 16)` — so **104 of the cluster's 176
bytes (59%) are sized entirely by `ShapedGlyph`** (RESEARCH §2.3). This is
the single most important structural fact in the whole plan: shrinking the
glyph shrinks the cluster twice over.

### 1.2 There are FOUR retained copies of the same shaped text

| # | where | type | site |
|---|---|---|---|
| 1 | `TextShapingCache.per_item_shaped` | `HashMap<u64, Arc<PerItemShapedEntry>>` → `Vec<ShapedItem>` | `cache.rs:6117`, `:6100-6105` |
| 2 | `TextShapingCache.shaped_items` | `HashMap<CacheId, Arc<Vec<ShapedItem>>>`, comment says *"monolithic, for backward compat"* | `cache.rs:6114` |
| 3 | `UnifiedLayout.items` on the layout node | `Vec<PositionedItem>`, each embedding a **cloned** `ShapedItem` | `cache.rs:10511`, held via `CachedInlineLayout.layout: Arc<UnifiedLayout>` (`layout_tree.rs:232`) on `LayoutNodeWarm.inline_layout_result` (`layout_tree.rs:503`) |
| 4 | `CachedInlineLayout.item_metrics` | `Vec<InlineItemMetrics>`, one per positioned item | `layout_tree.rs:250`, built at `:314-348` |

Plus two upstream stages that also retain per-run text:
`logical_items` (`cache.rs:6110`, `LogicalItem::Text` owns a `String`) and
`visual_items` (`cache.rs:6112`, `VisualItem` owns another `String`).

And a **fifth** copy, transient but per-frame and easy to miss:
`solver3/fc.rs:7542-7553` rebuilds *every* `PositionedItem` — cloning
every `ShapedItem` — purely to add a `y_offset` for `vertical-align`, then
builds a whole new `UnifiedLayout` at `:7555-7558`. Under the §3 model
this becomes one `f32` on the `LineRecord`.

### 1.3 The two ~19 MB passes, at source

MEASURED (§20, locale-corrected): `perform_fragment_layout` retains
**18.89 MB** and `shape_visual_items_with_per_item_cache` retains
**19.49 MB** — 38.4 of the 51.4 MB total (75%), split almost evenly.
Reading the code says exactly why:

**The measure pass.** `measure_intrinsic_widths` (`cache.rs:6830`) runs the
full stage 1→3 pipeline and stores the result in `self.shaped_items`
(`:6904-6916`) before scanning it for min/max widths (`:6932-6979`). The
*scan* is cheap; the *cache insert* is the 19.5 MB. Called from
`calculate_ifc_root_intrinsic_sizes` (`solver3/sizing.rs:709`).

**The layout pass.** `layout_flow` (`cache.rs:6592`) hits the same
`shaped_items` entry (`:6716-6718`, a pure `Arc` clone — so this half is
already shared) and then `perform_fragment_layout` (`cache.rs:9067`)
**deep-clones every item into a `PositionedItem`**:

    cache.rs:10511    positioned.push(PositionedItem {
    cache.rs:10512        item: item.clone(),

That clone copies a `String`, a `SmallVec`, and bumps two `Arc`s, per
cluster. That is copy #3.

**And the per-item cache hit clones too** — this is the finding that is not
in the report. `shape_visual_items_with_per_item_cache` (`cache.rs:7531`) on
a cache HIT does:

    cache.rs:7627    shaped.extend(cached.clusters.iter().map(|c| {
    cache.rs:7628        let mut c = c.clone();
    ...
    cache.rs:7647            sc.source_node_id = source_node_id;
    cache.rs:7648            for g in &mut sc.glyphs {
    cache.rs:7649                g.style = style.clone();

The comment at `:7606-7625` explains why: the cache key is `layout_hash`,
which excludes paint-only properties by design, but **paint (`style`) and
identity (`source_node_id`) are stored INSIDE the cluster**, so a shared
entry would hand back the wrong colour and mis-attribute damage. Two
ribbon tab headers with identical text hit one entry.

**This is the architectural defect, stated precisely: the shaping cache
cannot be shared because per-node paint and identity are interleaved with
per-cluster geometry.** Every fix below follows from separating them.

### 1.4 Where the bytes go, per cluster (INFERRED)

Engine accounting on the realistic corpus is 322 B/cluster for
`warm.inline` (§10). The struct arithmetic is:

    PositionedItem                       200 B
    InlineItemMetrics (parallel array)    32 B
    ShapedCluster::text heap (ASCII)     ~1-4 B counted
                                        -------
                                        ~233 B

    measured                             322 B
    implied Vec capacity factor  (322-32)/200 = 1.45

`UnifiedLayout.items` is built by `Vec::new()` + `push` (`cache.rs:9128`,
`:10511`), so it grows by doubling and carries ~45% slack.
`item_metrics` is built by `.collect()` from an exact-size iterator
(`layout_tree.rs:317-347`) and has none. **INFERRED: ~2.96 MB of
`warm.inline` on this document is Vec doubling slack**
(0.45 × 200 B × 32 937). The same applies to the `Vec<ShapedItem>` in
`shape_visual_items_with_per_item_cache`.

This is real memory, not accounting: both the engine and heaptrack
multiply by `.capacity()` (§12 lists the five sites).

### 1.5 Fields nobody reads

Established by exhaustive grep over `layout/src`, `core/src`, `dll/src`
(worktrees excluded), and confirmed by hand at the cited lines:

| field | bytes | verdict |
|---|---:|---|
| `ShapedCluster::is_first_fragment` / `is_last_fragment` | 2 | **DEAD.** Zero reads anywhere. Every `.is_first_fragment` hit in the repo belongs to `InlineBorderInfo` (`cache.rs:2991`, `:2997`) or `SimpleGlyphRun::border` (`glyphs.rs:196-205`). Written at `cache.rs:8590-8591`, `:8648-8649`, `:9965-9966`, `:10860-10861`, `glyphs.rs:607-608`, `knuth_plass.rs:785-786`, `:931-932`, `selection.rs:383-384`. |
| `ShapedCluster::source_content_index` | 8 | **Derivable.** One read outside shaping: `solver3/break_token.rs:325`. `solver3/paged_layout.rs:2165` explicitly notes it *"is not populated on the paged shaping path"* and reconstructs it from `source_cluster_id` at `:2169-2170` — so a derivation already exists in-tree. |
| `ShapedGlyph::kerning` | 4 | **Foldable into `advance`.** Every production read is the sum: `glyphs.rs:145`, `:418`, `:471`; `cache.rs:7794`, `:8423`. Read alone only in tests (`layout/tests/text3_shaping_exact.rs:61`, `test_glyph_cache_shaping.rs:97-194`). |
| `ShapedGlyph::vertical_advance` / `vertical_offset` | 12 | **Vertical-only.** Only consumer is the vertical-writing-mode fixup `cache.rs:8727-8742`, the hyphenation vertical branch `:9885`, and `into_glyph_instance` (`cache.rs:4726`) whose only reader of `vertical_offset` is itself — the display list uses `into_glyph_instance_at_simple` (`glyphs.rs:99`), which ignores writing mode. Zero on all horizontal text. |
| `ShapedGlyph::script` | ~4 (padded) | **1 bit.** Two reads, both Arabic: `cache.rs:10721` (kashida font selection), `:10894` (`is_arabic_cluster`). |
| `ShapedGlyph::font_metrics` | 32 | **Per-font, not per-glyph.** Six read sites (`cache.rs:5878`, `:8731`, `:8806`, `:8814`, `:8881`, `:8891`, `:10727`, `:9951`), all reachable from `font_hash` via `LoadedFonts`. `struct_sizes.rs:86-91` already flags it. |
| `ShapedGlyph::style` | 8 | **Per-run.** Read for `color`/`background`/`font_size_px`/`text_decoration` in `glyphs.rs:81-87` and `:317-321`, and `display_list.rs:4740`. `ShapedCluster::style` is the *same* `Arc` except across a font-fallback boundary inside one cluster. |
| `ShapedCluster::marker_position_outside` | 2 | **1 bit.** Three reads, all in line positioning: `cache.rs:10349`, `:10477`, `:10519`. |

**Must keep, in some form:**
`source_cluster_id` (the caret address — 30+ distinct read sites, and
`GraphemeClusterId` is FFI-frozen), `source_node_id` (hit-test tag +
damage attribution: `layout_tree.rs:322`, `glyphs.rs:152`,
`window.rs:9453`), `direction` (RTL selection geometry: `cache.rs:5005`,
`:5088`, `:5137`, `:5165`, `:5195`, `:5226`), `advance`, and the
information currently carried by `text`.

### 1.6 `ShapedCluster::text` — what it is actually used for

The doc comment says *"crucial for correct hyphenation"* (`cache.rs:4663`).
It is used for far more, and the pattern matters:

- **~25 sites re-derive UAX#14 line-break classes from it on every call** —
  `cache.rs:11590` (`is_cjk_character`), `:11611`/`:11636` (soft hyphen),
  `:11649` (`ends_with('-')`/`'‐'`/`'/'`), `:11771-11785`, `:11915-11935`,
  `:10973-10991` (word separators), `is_break_opportunity_with_word_break`
  and `is_word_separator` (both scan `c.text.chars()`). This is a UTF-8
  decode per predicate call, in the line breaker's inner loop.
- **Hyphenation** genuinely needs the bytes: `cache.rs:9873`, `:9881`,
  `:9893`.
- **Caret stops**: `cache.rs:5284` `grapheme_stops()` +
  `cluster_is_grapheme_continuation` at `:5301` (UAX#29 combining-mark
  folding).
- **Byte length**: `window.rs:9782` `byte_offset_to_cursor`.
- **PDF ToUnicode CMap**: `glyphs.rs:313`, `:335-360`, `:393`, `:411`.
- **Bidi plaintext detection**: `cache.rs:9174`.

**INFERRED, and it reframes the field:** most of those reads want a
*classification*, not the bytes. A precomputed flags word — which is
exactly what Gecko's `CompressedGlyph` carries — replaces them and is
*faster*, because the class is computed once at shaping time instead of
per line-break probe.

### 1.7 The API boundary already exists

`layout/src/text3/glyphs.rs` is the only place outside `text3` that walks
shaped items to produce a consumable view:

| function | line | consumer |
|---|---:|---|
| `get_glyph_runs_simple` | `glyphs.rs:57` | `solver3/display_list.rs:4767` — the paint path |
| `get_glyph_runs_pdf` | `glyphs.rs:286` | **printpdf** `src/html/bridge.rs:1031`, `:909` — PDF text + ToUnicode |
| `get_glyph_positions` | `glyphs.rs:445` | tests, and the reference implementation the other two agree with |

**printpdf 0.12.5 comes from crates.io** (`dll/Cargo.toml:169`) with its
`azul-*` dependencies path-repointed at this workspace
(`Cargo.toml:52-54`). Its only contact with shaped data is
`downcast_ref::<UnifiedLayout>()` (`bridge.rs:286`, `:518`) plus
`get_glyph_runs_pdf` — **no direct field access on `PositionedItem`,
`ShapedCluster` or `ShapedGlyph`** (verified by grep over
`printpdf/src`).

### 1.8 There is no FFI break

`api.json` contains **zero** occurrences of `ShapedGlyph`, `ShapedCluster`,
`ShapedItem`, `UnifiedLayout`, `PositionedItem` or `GlyphInstance`. What
*is* FFI-frozen and constrains us: `GraphemeClusterId` (`api.json:13693`,
defined `core/src/selection.rs:72` as `{source_run: u32,
start_byte_in_run: u32}`), `TextCursor`, `CursorAffinity`,
`SelectionRange`.

Two other hard constraints:
- `ShapedItem` is `#[repr(C, u8)]` deliberately, as a WASM-lift fix
  (`cache.rs:4581-4586`, and `LogicalItem` likewise at `:4453-4456`). Any
  replacement enum must keep an explicit tag at offset 0.
- `layout/src/font_traits.rs:195` and `:222` define **stub**
  `ShapedItem` / `UnifiedLayout` for the no-text-layout build. Keep in
  sync or that configuration stops compiling.

---

## 2. REGRESSION TESTS FIRST — the precondition

**Nothing in §4–§7 may start until this section is green.** This is the
user's explicit precondition and it is also the only defence against the
failure mode this refactor invites: shaped text is consumed by selection,
caret motion, hit-testing, PDF export, pagination and damage attribution,
and *most of those fail silently* — a wrong cluster id produces a
selection rect in the wrong place, not a panic.

### 2.1 What is already pinned — this work is ADDITIVE

Do not rewrite these. They are the reason the refactor is feasible at all.

| area | coverage | where |
|---|---|---|
| **Bidi / RTL geometry** | strong — 11 exact-x tests + RTL selection/caret | `layout/tests/text3_regression_bidi.rs`, `text3_brutal_shaping.rs`, `text3_brutal_selection.rs`, `text3_regression_selection_edit.rs`, `text3_selection_exact.rs` |
| **Line breaking (UAX#14 + CSS)** | strong — 15 + 25 tests | `layout/tests/text3_regression_breaking.rs`, `text3_brutal_shaping.rs`, unit tests in `cache.rs` |
| **Ligatures + kerning** | **golden**, literal glyph IDs and advances | `layout/tests/text3_shaping_exact.rs` (14), against `layout/tests/fonts/azul-mock-{liga,kern,arabic}.ttf`; `test_glyph_cache_shaping.rs` (vs `hb-shape`) |
| **Combining marks** | strong | `text3_brutal_shaping.rs`, `text3_brutal_selection.rs`, `cache.rs` `grapheme_stops` tests, `selection.rs` |
| **Whitespace + TABS** | strong at both levels | `layout/tests/text3_regression_whitespace.rs` (3 tab tests), `whitespace_processing.rs` (18), `knuth_plass.rs::convert_tab_is_glue_and_not_a_wrap_opportunity` |
| **Caret motion + cursor rects** | strong — exact byte offsets | `layout/tests/text3_cursor_exact.rs` (12), `text3_regression_selection_edit.rs` (19) |
| **Selection rectangles** | strong incl. multi-span and RTL | `layout/tests/text3_selection_exact.rs` (10), `text3_brutal_selection.rs` (15) |
| **Editing transforms** | 95 unit tests, pure | `layout/src/text3/edit.rs` |
| **contenteditable end-to-end** | 14 + 2 | `layout/tests/contenteditable_e2e.rs`, `vview_contenteditable_e2e.rs` |
| **Cross-block selection + `ReplaceChildren`** | 6 — the only `ReplaceChildren` assertions in the repo | `layout/tests/cross_block_selection.rs:192`, `:350` |
| **Knuth-Plass** | 49 | `layout/src/text3/knuth_plass.rs` |
| **Break-token pagination laws** | 5 + 9 + 28 | `layout/tests/break_token_pages.rs`, `solver3/break_token.rs`, `solver3/paged_layout.rs` |
| **IFC caching + `InlineItemMetrics`** | 18 | `layout/tests/ifc_caching.rs` |
| **Struct sizes** | 6 | `layout/tests/struct_sizes.rs` |
| **Reftests** | 52 cases, **45 baseline-green** | `doc/working/*.xht`, gate list `doc/reftest_baseline.txt`, harness `doc/src/reftest/mod.rs:170`, run with `cargo run -r -p azul-doc reftest` |

### 2.2 Dead tests that look like coverage — resolve these FIRST

Three bodies of test code do not run. Leaving them is worse than deleting
them, because a reader (or a future agent) counts them as protection.

| what | count | why it does not run |
|---|---:|---|
| `layout/tests/text3/{one..six}.rs` | **57 `#[test]`** | written as in-crate tests (`use crate::text3::…`), but `layout/src/text3/mod.rs` has no `mod tests` and `layout/Cargo.toml` has no `[[test]] path`. Cargo does not auto-discover `tests/<dir>/mod.rs`. **These are the ONLY tests for shape-outside exclusion layout (`test_layout_with_shape_exclusion` ×3) and for dictionary hyphenation (`test_hyphenation_break`, `test_hyphenation_break_2`).** |
| the whole `tests/` crate (`azul-test`) | ~34 | `exclude`d from the workspace (`Cargo.toml:24`) and in zero CI jobs. `tests/src/text-layout.rs` imports `azul_layout::text2::layout::split_text_into_words`; **`layout/src/text2` does not exist**, so the crate does not compile. |
| `layout/tests/selection.rs` | 5 | header says *"currently disabled pending API export"* |
| `layout/tests/solver3/test_inline_intrinsic_width.rs` | 2 | no `[[test]]` target |

**Task T0 (blocking): revive `layout/tests/text3/` or delete it.**
Reviving is preferred — it is 57 tests, and two of the areas it covers
(shape-outside, dictionary hyphenation) are otherwise unpinned. Add
`[[test]] name = "text3_suite" path = "tests/text3/mod.rs"` to
`layout/Cargo.toml` and rewrite `use crate::text3::` → `use
azul_layout::text3::`. Expect some to fail; that is information, and it is
information you want *before* the refactor, not during it.

### 2.3 New tests, in the order they must be written

Each one names the refactor step it protects. **Every one must be shown
RED against a deliberately broken build before it is trusted** — this
project has repeatedly shipped gates whose premise was the defect
(memory note `azul-gates-with-wrong-premises`).

---

**T1 — `layout/tests/text3_cluster_source_roundtrip.rs` (NEW)**
*Protects: every step. This is the single most important new file.*

Currently the cluster→source mapping is asserted only pointwise against
hand-written tables (`text3_selection_exact.rs:143-144`,
`text3_cursor_exact.rs:76-82`). There is **no property test**, and
`source_content_index` propagation is not pinned at all.

Pin, as properties over a corpus of ~20 documents (ASCII, Latin-1
accents, Hebrew, Arabic, CJK, mixed bidi, combining marks, ligature
text, emoji/astral, soft hyphens, tabs, `<br>`, nested spans, a list with
markers, a `text-combine-upright` run):

1. For every `ShapedItem::Cluster` in the layout:
   `byte_offset_to_cluster_id(map, c.source_cluster_id.start_byte_in_run)
   == c.source_cluster_id` (round-trip; `selection.rs` has only the
   forward direction).
2. Concatenating every cluster's source slice in `(source_run,
   start_byte_in_run)` order **reproduces the input text exactly**. This
   is the invariant that lets `ShapedCluster::text` be deleted.
3. `source_cluster_id` values are strictly increasing within a run and
   unique across the layout (bidi must not collide them —
   `VisualItem::run_byte_offset` at `cache.rs:4576` exists precisely to
   prevent this).
4. `shaped_item_source()` (`break_token.rs:325`) returns the same
   `ContentIndex` whether read from `source_content_index` or
   reconstructed the way `paged_layout.rs:2169-2170` does. **This is the
   gate that authorises deleting `source_content_index`.**
5. Every cluster's `source_node_id` matches the DOM node whose text
   produced it — including the two-identical-labels case (§1.3).

---

**T2 — `layout/tests/text3_shaping_cache_identity.rs` (NEW)**
*Protects: step 4 (killing the double retention). Highest-risk area.*

`cache.rs:7606-7654` is a correctness fix (commit `8ec9f387d`) with **no
test**. Any sharing scheme will re-break it and nothing will notice.

Pin:
1. Two nodes with identical text and identical `layout_hash` but
   **different `color`** produce display-list runs with different colours.
2. Same, but different `background_color` / `text_decoration`.
3. Same text, two nodes → two distinct `source_node_id`s in
   `get_glyph_runs_simple` output (`glyphs.rs:152`) → two distinct
   hit-test tags (`display_list.rs:4948`).
4. Geometry is byte-identical between the two (that IS the cache key).
5. Negative control: force the cache to return the first entry unmodified
   and require 1–3 to fail.

---

**T3 — `layout/tests/text3_pdf_extraction.rs` (NEW)**
*Protects: step 5 (deleting `ShapedCluster::text`). This is the PDF
text-extraction contract.*

`get_glyph_runs_pdf` (`glyphs.rs:286`) is consumed by an external
published crate (§1.7). Pin its output as a contract, not an
implementation detail:
1. Concatenating `PdfGlyphRun::cluster_texts` in run order reproduces the
   source text of the IFC exactly, for the T1 corpus.
2. `cluster_texts` stays parallel to `glyphs` (there is a unit test for
   this in `glyphs.rs`; promote it to the integration corpus).
3. Ligature case: `ffi` → one glyph, `unicode_codepoint == "ffi"`.
4. Combining-mark case: base+mark → the full grapheme, not the base.
5. Astral case: an emoji's codepoint survives (`glyphs.rs` documents a
   **panic** at `pdf_cluster_offset_inside_multibyte_char_panics` — pin
   the current behaviour so a change is deliberate).
6. `get_glyph_runs_simple` and `get_glyph_positions` agree with
   `get_glyph_runs_pdf` on glyph id + absolute x for every cluster.

---

**T4 — `layout/tests/text3_memory_budget.rs` (NEW)**
*Protects: the whole point. Makes the win non-reversible.*

`struct_sizes.rs` pins struct sizes but nothing pins the **aggregate**.
Lay out a fixed 200-line document with the mock font and assert:
1. `LayoutTree::memory_report().warm_inline_layout_bytes / shaped_cluster_count`
   ≤ a constant, updated deliberately at each step of §7.
2. `TextShapingCache::memory_report().bytes_per_cluster()` ≤ a constant
   (this accessor already exists, `cache.rs:6199`).
3. `items.capacity() == items.len()` for every retained
   `UnifiedLayout` — the capacity-slack gate for step 1.
4. `distinct_style_arcs` stays in single/low-double digits, not tracking
   the glyph count. This is the Stylo failure mode (RESEARCH §4.6:
   109k `ComputedValues` where 2 200 were expected) and the engine
   already prints it (`window.rs:3169-3175`) — 419 distinct arcs for
   32 937 glyphs today (§10), ~40x more than a document has styles.

---

**T5 — `layout/tests/text3_vertical_geometry.rs` (NEW)**
*Protects: steps 3 and 5 (moving vertical fields to a side table).*

Vertical writing modes have **one** pixel-presence smoke test
(`text3_visual.rs:208 vertical_rl`) and **no geometry assertion**.
`layout/tests/fonts/azul-mock-vertical.ttf` exists and **no test loads
it**. Pin exact positions for `writing-mode: vertical-rl` and
`vertical-lr`: per-cluster y advance, line advance in x, and the
`apply_text_orientation` fallback path at `cache.rs:8727-8742` (zero
`vertical_advance` → `line_height`).

---

**T6 — `layout/tests/text3_combined_block.rs` (NEW)**
*Protects: the `ShapedItem::CombinedBlock` arm through steps 2–5.*

Tate-chu-yoko is **essentially unpinned**: 3 unit tests in `glyphs.rs`,
zero for the CSS→pipeline path (`solver3/fc.rs:4770-4785`,
`solver3/getters.rs:6190`, `cache.rs:4152`). `struct_sizes.rs:207`
already exists purely to force a look at this arm. Pin: a
`text-combine-upright: all` run inside vertical text produces one
`CombinedBlock`, its glyphs do not stack at one x, its bounds are one em
square, and it survives line breaking as an atomic unit.

---

**T7 — extend `layout/tests/pagination_dom_breaks.rs` and
`drag_image_between_pages_e2e.rs`**
*Protects: steps 4 and 5 against the paged shaping path.*

Per-page exclusions have **one** live test
(`drag_image_between_pages_e2e.rs:133`); the three
`test_layout_with_shape_exclusion` are in the dead `layout/tests/text3/`.
`PageSequence` has **zero** test hits anywhere. Add: a document whose
pagination is recomputed after an edit produces the same break bytes;
exclusions on page N do not leak to page N+1; and
`mid_paragraph_break_exposes_the_line_start_byte` still holds when the
cluster record changes shape.

---

**T8 — text selection and typing in the `e2e/` JSON corpus**
*Protects: the integration path CI actually runs.*

`e2e/` has 38 scenarios and **none performs text selection, drag-select,
or typed-character input** — `op-caret-blink.json` sends a key pair only
to reset blink phase. Add three: click-drag-select a paragraph and assert
selection rect count + bounds; type five characters into a
contenteditable and assert damage is bounded to the line; select across
two blocks and copy. Note `layout/tests/e2e_json.rs` runs the whole
corpus, and `doc/src/gene2e.rs:382` `OP_POLICY` needs a classification row
for any new op (memory note: this gate lives in `cargo test -p azul-doc`,
which is **not** in the local battery).

---

**T9 — reftest additions**
*Protects: pixel-level output.*

Three of the seven currently-failing reftests are inline/text cases
(`inline-block-text-001`, `inline-elements-001`, `inline-background-001`).
Do **not** try to fix them as part of this work. Do add two new `.xht`
cases to `doc/working/` and to `doc/reftest_baseline.txt` in the same
commit (the file header requires this): one for a paragraph with mixed
inline styles and a wrap, one for RTL text with a selection-free
baseline. `doc/xhtml1/` holds 9 629 upstream W3C CSS2.1 files that are
**not wired into the runner** — a cheap expansion surface if more pinning
is wanted, but out of scope here.

### 2.4 The gate command

    cargo test -p azul-css -p azul-core -p azul-layout --lib --tests \
      --features azul-core/serde-json,azul-layout/json,azul-layout/e2e-server
    cargo test -p azul-layout --test e2e_json --features e2e-server
    cargo run -r -p azul-doc reftest        # must stay >= 45/52, by NAME
    cargo test -p azul-doc --bins           # the OP_POLICY gate

Release mode only (`--release` / `-r`) — user rule, memory note
`azul-release-only-builds`. `timeout 600 cargo test` always
(`cargo-test-timeout`).

---

## 3. The target data model

### 3.1 The principle

Gecko: a **compact fixed-size record per character** (4-byte
`CompressedGlyph`, *"no virtual methods or destructor, and just a single
`uint32_t` data member"*) plus a **`DetailedGlyph` side table** for the
characters that do not fit the compact form; 6 B/char retained including
the UTF-16 text (RESEARCH §4.2).

Skia: cluster indices are *"not needed to correctly draw the glyphs"* and
exist only for selection/editing — separately allocatable. Its allocation
ladder is `allocRun` (0 scalars/glyph) / `allocRunPosH` (1) /
`allocRunPos` (2), and horizontal single-baseline runs are the cheapest
rung — which is what nearly all of our runs are.

Blink: NGFragmentItem replaced a redundant fragment *tree* with a **flat
list** and took Wikipedia 1.6 MB → 0.3 MB (82%), one line box 640 → 152 B
(RESEARCH §4.4). Same team measured that `sizeof(Node)` *"doesn't really
matter"*. **Structure beats field-shrinking.**

Applied here: **separate what varies per cluster from what varies per
run, and keep only the per-cluster part in the dense array.**

### 3.2 The types

```rust
/// One per shaped cluster. Dense, POD, no Drop glue, no owned heap.
/// 16 bytes. Compare Gecko's 4-byte CompressedGlyph + 2-byte char.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct ClusterCompact {
    /// Glyph to draw when `flags & HAS_DETAIL == 0`.
    pub glyph_id:   u16,          // 2
    /// Precomputed classification — replaces ~25 re-derivations from
    /// `text` (§1.6). See ClusterFlags below.
    pub flags:      u16,          // 2
    /// Total advance INCLUDING kerning, as painted (§1.5).
    pub advance:    f32,          // 4
    /// == GraphemeClusterId::start_byte_in_run. The run supplies
    /// `source_run`, so the FFI-frozen id reconstructs exactly.
    pub start_byte: u32,          // 4
    /// Inline-axis position within the IFC. `y` comes from the line.
    pub x:          f32,          // 4
}                                 // = 16
```

```rust
bitflags ClusterFlags: u16 {
    IS_BREAK_OPPORTUNITY   // is_break_opportunity_with_word_break, precomputed
    IS_WORD_SEPARATOR      // is_word_separator
    IS_NO_BREAK_SPACE      // GL/WJ: NBSP, U+202F, U+2060, U+FEFF
    IS_ZERO_WIDTH_SPACE
    IS_SOFT_HYPHEN
    ENDS_WITH_HYPHEN       // '-' | U+2010 | '/'  (break-after, cache.rs:11649)
    IS_CJK                 // is_cjk_cluster
    IS_ARABIC              // replaces ShapedGlyph::script (2 read sites)
    IS_GRAPHEME_CONTINUATION  // UAX#29, for grapheme_stops (cache.rs:5301)
    IS_NOTDEF              // GlyphKind::NotDef
    IS_HYPHEN_GLYPH        // GlyphKind::Hyphen (line-breaker inserted)
    IS_RTL                 // ShapedCluster::direction
    IS_MARKER_OUTSIDE      // marker_position_outside == Some(true)
    HAS_DETAIL             // -> ClusterDetail side table
    HAS_VERTICAL           // -> VerticalDetail side table
}
```

```rust
/// One per shaped RUN. ~792 runs for 32 937 clusters on the measured
/// document, i.e. one per ~42 clusters.
pub struct ShapedRun {
    pub style:           Arc<StyleProperties>,   // 8
    pub font_hash:       u64,                    // 8
    pub font_metrics:    LayoutFontMetrics,      // 32, ONE copy per run
    pub source_run:      u32,                    // 4
    pub source_node_id:  Option<NonMaxU32>,      // 4  (see §3.4)
    pub text:            Arc<str>,               // 16, shared with VisualItem
    pub clusters:        Range<u32>,             // 8
    pub script:          Script,                 // 1 + pad
}                                                // ~ 88 B
```

```rust
/// Only for clusters with HAS_DETAIL: ligatures, combining marks,
/// multi-glyph clusters, GPOS-offset glyphs, kashida.
pub struct ClusterDetail {
    pub cluster: u32,             // index into the dense array
    pub glyphs:  Range<u32>,      // into a shared DetailGlyph array
}
#[repr(C)] #[derive(Copy, Clone)]
pub struct DetailGlyph {
    pub glyph_id:       u16,
    pub cluster_offset: u16,
    pub advance:        f32,      // incl. kerning
    pub offset:         Point,    // 8
}                                 // = 16
```

```rust
/// Replaces PositionedItem::line_index (8 B/cluster) and position.y
/// (4 B/cluster) with per-LINE data. Also turns get_selection_rects'
/// O(all items) line filter (cache.rs:5014, :5062) into O(line).
pub struct LineRecord {
    pub clusters:  Range<u32>,
    pub baseline_y: f32,
    pub top_y:     f32,
    pub height:    f32,
}
```

```rust
pub struct UnifiedLayout {
    pub clusters: Box<[ClusterCompact]>,   // exact capacity, no slack
    pub runs:     Box<[ShapedRun]>,
    pub lines:    Box<[LineRecord]>,
    pub details:  Box<[ClusterDetail]>,    // empty on plain Latin
    pub detail_glyphs: Box<[DetailGlyph]>,
    pub vertical: Box<[VerticalDetail]>,   // empty for horizontal WM
    pub atomics:  Vec<AtomicItem>,         // Object / CombinedBlock / Tab / Break
    pub overflow: OverflowInfo,
}
```

`AtomicItem` keeps the non-cluster arms of today's `ShapedItem`
(`Object`, `CombinedBlock`, `Tab`, `Break` — `cache.rs:4591-4612`) as a
sparse `Vec`, since they are rare and heterogeneous. Keep it
`#[repr(C, u8)]` per §1.8.

### 3.3 Disposition of every current field

| current field | bytes | goes to |
|---|---:|---|
| `ShapedCluster::text` | 24 + heap | **DELETED.** Derived: `run.text[start_byte .. next.start_byte]`. Classification moved to `flags`. |
| `source_cluster_id` | 8 | `start_byte` (4) + `run.source_run` (amortised) |
| `source_content_index` | 8 | **DELETED.** Reconstructed as `paged_layout.rs:2169` already does. |
| `source_node_id` | 16 (`Option<NodeId>`, no niche) | `run.source_node_id`, 4 B, amortised |
| `glyphs: SmallVec<[ShapedGlyph;1]>` | 104 | `glyph_id` (2) for the common case; side table otherwise |
| `advance` | 4 | `advance` (4), now incl. kerning |
| `direction` | 1 | `IS_RTL` flag bit |
| `style: Arc` | 8 | `run.style`, amortised |
| `marker_position_outside` | 2 | `IS_MARKER_OUTSIDE` flag bit |
| `is_first_fragment`/`is_last_fragment` | 2 | **DELETED** (dead, §1.5) |
| `ShapedGlyph::kind` | 8 | `IS_NOTDEF` / `IS_HYPHEN_GLYPH` flags; `Kashida{width}` → detail table |
| `ShapedGlyph::cluster_offset` | 4 | detail table only |
| `ShapedGlyph::kerning` | 4 | folded into `advance` |
| `ShapedGlyph::offset` | 8 | detail table only (zero for most glyphs) |
| `ShapedGlyph::vertical_*` | 12 | `VerticalDetail`, vertical WM only |
| `ShapedGlyph::script` | 4 | `IS_ARABIC` flag + `run.script` |
| `ShapedGlyph::font_hash` | 8 | `run.font_hash` |
| `ShapedGlyph::font_metrics` | 32 | `run.font_metrics` |
| `ShapedGlyph::style` | 8 | `run.style` |
| `PositionedItem::position` | 8 | `x` (4) + `line.top_y` |
| `PositionedItem::line_index` | 8 | `LineRecord.clusters` range (binary search) |
| `InlineItemMetrics` | 32 | **DELETED.** `advance_width` == `cluster.advance`, `x_offset` == `cluster.x`, `line_index` == the line range, `source_node_id` == `run.source_node_id`; only `line_height_contribution` and `can_break` are genuinely new (RESEARCH §2.3) and both derive from `flags` + the line record. |

### 3.4 One prerequisite that is not text3's

`NodeId` (`core/src/id.rs:71`) is `{ inner: usize }` with **no niche**,
despite its own doc comment describing a 1-based encoding where 0 means
None. `Option<NodeId>` therefore costs 16 B. A `NonMaxU32` newtype makes
it 4. This is a `azul-core` change with repo-wide blast radius; the plan
uses `Option<NonMaxU32>` locally in `ShapedRun` and converts at the
boundary rather than taking that on.

### 3.5 Expected bytes per cluster (INFERRED — arithmetic shown)

Denominators. **32 937 clusters is MEASURED** (§10, the realistic
corpus). **792 runs is INFERRED** — it is the stage-cache entry count
measured on `big.md` (`cache.rs:6125` records it in a comment) and the
realistic corpus's 419 distinct style Arcs (§10) is the same order;
treat it as ±2x. **~24 clusters per line is INFERRED** from
32 937 clusters over a wrapped 960-line document. Both amortised terms
are small enough that a 2x error moves the total by ~1.5 B/cluster.

    ClusterCompact                          16.00 B
    ShapedRun,  88 B / 42 clusters           2.10 B
    LineRecord, 24 B / 24 clusters-per-line  1.00 B
    ClusterDetail + DetailGlyph
      at 2% incidence, ~32 B each            0.64 B
    VerticalDetail (horizontal doc)          0.00 B
    ------------------------------------------------
    dense total                             19.74 B/cluster

    + Box<[T]> has exact capacity, so 0% slack
    + Arc<str> run text: the source text is ALREADY retained in
      `logical_items`/`visual_items` (cache.rs:6110, :6112); sharing it
      adds 0 and lets those two caches stay as they are.

    working budget, with headroom            24 B/cluster

Against the two current figures:

    engine accounting   ≤853 B  ->  24 B     = 97.2% reduction, 35.5x
    allocator bytes    1 460 B  ->  24 B     = 98.4% reduction, 60.8x
    vs Gecko's 6 B/char                        4.0x  (was ~140-240x)

**Why 4x Gecko and not 1x, honestly:** Gecko does not store `x` per
character (it accumulates advances at paint time) and its 6 B counts a
2-byte UTF-16 char that we get for free from a shared `Arc<str>`. Dropping
`x` would take us to 12 B/cluster at the cost of an O(line) accumulation
in `get_selection_rects` and `hittest_cursor`. **Do not do this in the
first pass** — it trades a measured memory win for an unmeasured
interactive-latency risk, and 16 B is already the win.

### 3.6 Total expected saving (INFERRED)

Retained shaped text at 960 realistic lines:

    32 937 clusters x 24 B                    = 0.79 MB dense
    + run/line/detail tables (in the 24)      = included
    + transient shaping buffers (not retained)
    + the upstream logical/visual caches       ~0.9 MB (unchanged, MEASURED)
    ------------------------------------------------------------
    retained shaped text, projected            ~2 MB

    today (allocator, text3::cache less
    FontManager)                               48.09 MB
    saving                                    ~46 MB

Conservative band, because ~9.9 MB of the 48.09 has a backtrace owner but
no object owner (§14) and may not live in the structures being shrunk:

    **-35 to -46 MB** off a 145.4 MB process = **145.4 -> 99-110 MB**

Scaling law (the more useful number). MEASURED:
`RSS(MB) = 78.5 + 0.0697 x lines`, flat to 3 840 lines (§7u). Shaped text
is 51.4 of the document's 72.3 MB RSS delta, so it carries
`51.4/72.3 x 69.7 = 49.6 kB/line` of the slope. At ~4% of its current
cost that becomes ~2 kB/line:

    projected  RSS(MB) = 78.5 + 0.022 x lines
      960 lines   145.4 ->  ~100 MB
     3 840 lines  346.0 ->  ~163 MB
    10 000 lines  ~776  ->  ~299 MB

**The intercept does not move.** 78.5 MB of fixed floor (15.6 buffers +
14.0 binary + 10.8 shared libs + 8.5 font files + 22.0 heap/anon) is
untouched by this work. **If the goal is "under 100 MB on a real
document", this refactor alone reaches the boundary and no further** —
see §6.5.

---

## 4. Killing the double retention

MEASURED: `perform_fragment_layout` 18.89 MB and
`shape_visual_items_with_per_item_cache` 19.49 MB, 75% of the total,
split 0.97:1 (§20). §1.3 located both at source. Three options.

### Option A — reference instead of clone (`PositionedItem` holds an index)

Make the positioned array a parallel array of `(cluster_index, x)` into
the `Arc<Vec<ShapedItem>>` the cache already owns, instead of embedding a
clone (`cache.rs:10511`).

- **Saves:** copy #3, ~10.6 MB of `warm.inline` (MEASURED) plus the
  app-side pagination's equivalent.
- **Blocked by, and this is the reason it has not been done:** line
  breaking *mutates* clusters. Hyphenation splits a cluster and inserts a
  hyphen glyph (`cache.rs:9942-9966`), justification rewrites advances and
  inserts kashida (`cache.rs:10828-10861`), `apply_text_orientation`
  rewrites every cluster for vertical writing modes
  (`cache.rs:8707-8742`), and `fc.rs:7542-7557` rebuilds every
  `PositionedItem` to apply a vertical-align offset.
- **Verdict: viable only with a copy-on-write override table** — which is
  exactly the `ClusterDetail` side table of §3.2. So Option A is not an
  alternative to the compact model; it is a consequence of it.

### Option B — delete the `shaped_items` monolithic cache

It is labelled *"monolithic, for backward compat"* (`cache.rs:6113`) and
`per_item_shaped` (`:6117`) is the incremental path that supersedes it.

- **Saves:** copy #2. UNKNOWN how much — `memory_report` folds
  `shaped_items_bytes` and `per_item_shaped_bytes` into separate fields
  (`cache.rs:6157`, `:6161`), so the split *is* measurable today; run
  `AZ_PROFILE=memory` and read it before deciding.
- **Costs:** `layout_flow` and `measure_intrinsic_widths` both take the
  monolithic hit as a pure `Arc` clone (`cache.rs:6716-6718`,
  `:6904`) — the *cheapest* path in the system. Deleting it forces both
  through the per-item path, which currently **deep-clones on every hit**
  (`cache.rs:7627-7654`). That would make things *worse*, not better.
- **Verdict: do not do this until the per-item hit stops cloning (§4/C).**
  Then re-measure and decide.

### Option C — stop the per-item cache hit from cloning (RECOMMENDED FIRST)

`cache.rs:7627-7654` clones every cached cluster to re-stamp `style` and
`source_node_id`. With the §3 model those two fields live in `ShapedRun`,
not in the cluster, so a cache hit becomes:

    Arc::clone(cached.clusters)          // share the dense array
    + push one ShapedRun with THIS node's style/source_node_id

Zero per-cluster work. **This is the largest single lever in the plan** and
it is a *structural* fix, not a field-shrink — the class of change Blink
measured at 82% (RESEARCH §4.4).

- **Saves:** most of the 19.49 MB attributed to
  `shape_visual_items_with_per_item_cache`, plus the CPU of ~33 000
  `String` clones and ~66 000 atomic `Arc` operations per document.
- **Risk:** it re-opens the exact defect `8ec9f387d` fixed. **T2 is the
  gate and must be written and shown red first.**

### Recommended combination

C (share the cache) → A (reference from the positioned array, with the
detail table as the CoW escape hatch) → re-measure → B only if
`shaped_items_bytes` is still material.

---

## 5. The API boundary — rich behaviour stays reachable, GUI is compact by default

### 5.1 The boundary already exists and must be frozen

§1.7: `layout/src/text3/glyphs.rs` is the only external walker of shaped
data, and **printpdf 0.12.5 (a published crate, path-repointed at this
workspace) depends on `get_glyph_runs_pdf` compiling unchanged.**

**RULE: `get_glyph_runs_simple`, `get_glyph_runs_pdf`,
`get_glyph_positions` and their result types (`SimpleGlyphRun`,
`PdfGlyphRun`, `PdfPositionedGlyph`, `PositionedGlyph`) are a FROZEN
cross-crate contract for the duration of this refactor.** Everything
behind them may change freely. Breaking them requires a coordinated
printpdf release, which memory note
`rust-fontconfig-4.5-release-chain` shows is a multi-week chain.

`UnifiedLayout` must also keep its *name and identity*, because printpdf
does `downcast_ref::<UnifiedLayout>()` on the type-erased
`DisplayListItem::TextLayout { layout: Arc<dyn Any + Send + Sync> }`
(`display_list.rs:717`, `bridge.rs:286`). Its *fields* are free.

### 5.2 The three tiers

**Tier 1 — retained, compact, always.** `UnifiedLayout` as in §3.2. This
is what the GUI holds. There is no flag and no opt-out: the compact form
is not lossy, so nothing needs to opt out of it.

**Tier 2 — borrowed views, zero allocation.** For code inside the engine
that used to pattern-match `ShapedItem::Cluster(c)`:

```rust
impl UnifiedLayout {
    pub fn cluster(&self, i: usize) -> ClusterView<'_>;
    pub fn clusters_on_line(&self, line: usize) -> ClusterViewIter<'_>;
}
pub struct ClusterView<'a> { /* &layout, cluster idx, resolved run idx */ }
impl<'a> ClusterView<'a> {
    pub fn text(&self) -> &'a str;                 // slice of run.text
    pub fn source_cluster_id(&self) -> GraphemeClusterId;
    pub fn source_node_id(&self) -> Option<NodeId>;
    pub fn style(&self) -> &'a Arc<StyleProperties>;
    pub fn font_metrics(&self) -> &'a LayoutFontMetrics;
    pub fn glyphs(&self) -> GlyphIter<'a>;         // inline or detail
    pub fn advance(&self) -> f32;
    pub fn is_rtl(&self) -> bool;
}
```

Every one of the ~45 `source_cluster_id` read sites, the selection and
caret code (`cache.rs:4891`, `:4970`, `:5184`, `:5247`, `:5277`, and
`move_cursor_*` at `:5394`–`:5849`), `selection.rs:15`/`:56`,
`layout_tree.rs:314`, `break_token.rs:325` and `paged_layout.rs:2163`
migrate to this. No behaviour change, no allocation.

**Tier 3 — materialised rich form, on demand, by function.** For callers
that genuinely want the old owned types:

```rust
/// Rebuild the pre-refactor representation. Allocates. Not retained by
/// the engine. This is what PDF export, text extraction and any external
/// consumer of cluster->source mapping should call.
pub fn to_shaped_items(layout: &UnifiedLayout) -> Vec<PositionedItem>;
```

`get_glyph_runs_pdf` (§5.1, frozen) is reimplemented directly against
Tier 2 — it does not need Tier 3, because everything it reads
(`cluster.text`, `glyph.style.color`, `glyph.font_hash`,
`glyph.style.font_size_px`, `text_decoration`, `line_index`,
`cluster.direction`, `style.writing_mode`, `glyph.offset`) is available
as a view accessor. `to_shaped_items` exists for third-party callers and
as the migration shim (§7 step 5), and should be documented as *"builds a
copy; do not hold it"*.

### 5.3 What this means for cluster→source mapping

The user's requirement — pdf2html/printpdf need cluster→source for PDF
text extraction — is satisfied **by the compact form itself**, not by an
opt-in. `ClusterCompact.start_byte` + `ShapedRun.source_run` reconstruct
`GraphemeClusterId` exactly, and `ShapedRun.text` gives the bytes. The
compact record is *smaller*, not *poorer*. That is the whole point of the
Gecko model, and it is why "keep the rich form for PDF" is a shim for
source compatibility rather than a data requirement.

---

## 6. Migration sequence

Each step is independently landable, independently measurable, and has a
gate that must be green before the next starts. **Steps are cumulative;
their savings are NOT additive** — the report's own repeated defect was
adding overlapping filter totals (§9, §13, §19). The table gives a
cumulative target per step, not a delta.

### Step 0 — tests (§2). Gate: T0–T9 written, each shown RED first, then green. Memory delta: 0.

### Step 1 — exact capacity on the retained vectors

`into_boxed_slice()` / `shrink_to_fit()` on `UnifiedLayout.items` at
construction (`cache.rs:9128` builds it by `push`) and on the
`Vec<ShapedItem>` in `shape_visual_items_with_per_item_cache`. Do **not**
touch per-cluster allocations — that is a no-op under glibc
(RESEARCH §6).

- **INFERRED saving:** 0.45 × 200 B × 32 937 = 2.96 MB from `warm.inline`
  alone; similar order from the shaping cache. Band **3–6 MB**.
- **Gate:** T4/3 (`items.capacity() == items.len()`);
  `warm.inline` B/cluster **322 → ≤240**; full battery + 45/52.
- **Why first:** zero semantic risk, and it removes a confound from every
  later measurement.

### Step 2 — delete the dead and redundant fields

Delete `is_first_fragment`, `is_last_fragment`, `source_content_index`
(with the `break_token.rs:325` reconstruction from §1.5). Collapse
`marker_position_outside` into a `flags: u8`. Fold `kerning` into
`advance` at the shaping site (`default.rs:936`, `:960`).

- **INFERRED saving:** ~24 B off `ShapedCluster` and 4 off `ShapedGlyph`;
  across three retained copies ≈ 72 B/cluster × 32 937 × 1.45 ≈ **3.4 MB**.
- **Gate:** T1/4 (the `source_content_index` reconstruction property);
  `struct_sizes.rs` updated **with the reason**, per its own header;
  `text3_shaping_exact.rs` and `test_glyph_cache_shaping.rs` updated for
  the kerning fold (they are the only readers of `kerning` alone).

### Step 3 — `ShapedGlyph` 96 B → ≤24 B

Move `font_metrics`, `font_hash`, `style`, `script` to a per-run table
indexed by a `u16` on the cluster; move `vertical_advance` /
`vertical_offset` to a vertical-only side table.

- **This pays twice.** The `SmallVec<[ShapedGlyph; 1]>` inline arm is
  `max(size_of::<ShapedGlyph>(), 16)`, so 96 → 24 collapses the inline
  arm 104 → 32 and takes `ShapedCluster` ~176 → ~80 and `PositionedItem`
  200 → ~112 (RESEARCH §2.3, INFERRED).
- **INFERRED saving:** ~268 B/cluster across the retained copies ×
  32 937 × 1.45 ≈ **12.8 MB**. Band **10–13 MB**.
- **Gate:** T5 (vertical geometry — this step is where vertical breaks
  silently); T2 (the style re-stamp now writes one run field, not N glyph
  fields); `struct_sizes.rs` `ShapedGlyph` 96 → ≤24; `warm.inline`
  B/cluster ≤ 200.
- **Confidence caveat:** BlinkOn measured that `sizeof` *"doesn't really
  matter"* (RESEARCH §4.4). This step is the low-confidence one. It is
  ordered before step 4 only because it is mechanically simpler and
  because it is a prerequisite for the run table that step 4 needs. **If
  its measured saving is under 5 MB, do not spend more time on
  field-shrinking — go straight to step 4.**

### Step 4 — share the shaping cache; stop cloning on hit

The §4/C fix, now possible because step 3 moved `style` and
`source_node_id` into the run. `cache.rs:7627-7654` becomes an
`Arc::clone` of the cluster array plus one run push.

- **INFERRED saving:** most of the 19.49 MB attributed to
  `shape_visual_items_with_per_item_cache` (MEASURED, §20). Band
  **10–16 MB**. Also removes ~33 000 `String` clones and ~66 000 atomic
  refcount ops per document — expect a *speed* improvement, and measure
  it (`layout/tests/frame_perf.rs`, `pagination_perf.rs`).
- **Gate:** **T2 is mandatory and must have been shown red.** This step
  re-opens defect `8ec9f387d` by construction. Plus full battery + 45/52
  + T7 (paged path) + T8 (e2e selection).
- **This is the highest-value step in the plan.**

### Step 5 — the compact record

`ClusterCompact` + `ShapedRun` + `LineRecord` + side tables (§3.2).
`PositionedItem` and `InlineItemMetrics` disappear. `ShapedCluster` and
`ShapedGlyph` survive only as Tier-3 materialised types (§5.2).
`ShapedCluster::text` is deleted; the ~25 line-break predicates read
`flags`; hyphenation, PDF ToUnicode and `grapheme_stops` slice
`run.text`.

- **INFERRED saving:** everything remaining, to the ~24 B/cluster budget
  of §3.5. Cumulative band **-35 to -46 MB**.
- **Gate:** all of T1–T9, full battery, 45/52 reftests, and
  `cargo test -p azul-doc --bins`. Plus a *speed* check — this is the
  step where a per-call `run.text` slice could regress the line breaker
  if the flags are not complete.
- **Sub-order within the step** (each independently landable):
  5a `LineRecord` (deletes `line_index` + `position.y`, and makes
  `get_selection_rects`' O(all-items) line filter at `cache.rs:5014`
  O(line));
  5b `ShapedRun` table + `ClusterView`;
  5c `ClusterFlags` + delete `ShapedCluster::text`;
  5d `ClusterCompact` dense array + detail tables;
  5e delete `InlineItemMetrics`.

### The ladder, with gate numbers

| step | `ShapedGlyph` | `ShapedCluster` | B/cluster (engine) | `text3::cache` on `uniq-960.md` | cumulative MB saved |
|---|---:|---:|---:|---:|---:|
| today | 96 | 176 | ≤853 | **44.78 M** (baseline, `rss-baseline.sh` header) | 0 |
| 1 | 96 | 176 | ≤640 | ≤41 M | 3–6 |
| 2 | 92 | ~152 | ≤590 | ≤39 M | 6–9 |
| 3 | ≤24 | ~80 | ≤330 | ≤29 M | 16–22 |
| 4 | ≤24 | ~80 | ≤200 | ≤18 M | 26–38 |
| 5 | — | — | **~24** | **≤6 M** | **35–46** |

Every figure in the last three columns is **INFERRED**. **Rule: if a
step's measured saving is under 50% of its projection, stop and re-derive
before continuing.** The report's own history is that projections built on
one unchecked input are wrong by 155% (§20).

---

## 7. Re-baselining — how to measure each gate

### 7.1 The procedure

    scripts/rss-baseline.sh gen              # deterministic 100%-distinct corpus
    scripts/rss-baseline.sh rss  <file.md>   # settled RSS + mapping breakdown
    scripts/rss-baseline.sh heap <file.md>   # heaptrack + per-owner filter totals

### 7.2 WARNING — the script's corpus is NOT the report's corpus

**Do not compare any `rss-baseline.sh` reading against a number from
`RSS_MAP_2026_08_07.md`.**

| | non-empty lines | VmHWM |
|---|---:|---:|
| `doc-uniq.md` — what §§7s-20 measured (scratchpad, **not durable**) | 640 | **145.4** |
| `uniq-960.md` — what `gen` produces (committed, reproducible) | 786 | **147.9** |

Both are 960 total lines and 100% distinct; the structural mix differs.
**The 2.5 MB between them is CORPUS, NOT REGRESSION.** Someone who runs
`gen`, reads 147.9 and compares it with the report's 145.4 will see a
2.5 MB regression that does not exist. **Baseline and re-measure with the
script's corpus and compare those two readings to each other.**

### 7.3 The committed baseline to measure against

From the `rss-baseline.sh` header (azul `743eb5837`, 1280x800, CPU):

    lines  non-empty        VmHWM        [heap]
      240        197     96 152 kB     41.5 MB
      960        786    147 860 kB     90.6 MB
     1920       1571    217 088 kB    164.3 MB

    fit   RSS(MB) = 78.7 + 0.0721 x lines

`heap` on `uniq-960.md`: peak 100.19 M, leaked 87.87 M,
**`text3::cache` 44.78 M / 81 sites**, `compute_document_pagination`
19.98 M / 87, `solver3::sizing` 19.67 M / 39, `ParsedFont` 13.06 M / 70,
`LayoutTreeBuilder` 4.78 M / 4.

*(The figures in memory note `azul-rss-mapping-job` — text3 42.14 /
pagination 18.53 — predate the last re-measure and are stale. The script
header is authoritative.)*

**These filters OVERLAP by construction.** Pagination and sizing both
reach `text3`; font decode is reached from `text3`'s `FontManager`. **Do
not add them.** Track each against its own previous value.

### 7.4 Read the slope and the intercept separately

The 78.7 MB intercept reproduces across three independently generated
corpora (78.5 / 78.7 / 79.3). **A change in the INTERCEPT is a change to
fixed cost; a change in the SLOPE is a change to per-line cost.** Every
step in §6 should move the **slope only**. If a step moves the intercept,
something unintended happened — investigate before proceeding. Measure at
240 / 960 / 1 920 lines and refit; a single RSS total hides which moved.

### 7.5 The engine's own report

`AZ_PROFILE=memory` prints, per layout (`window.rs:3144-3214`):
`warm.inline` KiB, clusters, glyphs, **B/cluster**, distinct style Arcs,
`sizeof ShapedItem/ShapedCluster/ShapedGlyph`, and
`TextShapingCache` split into `shaped_items` / `glyph_bytes` /
`cluster_text` / `per_item_shaped` / `map_overhead` / `style_arcs` /
B/cluster. Two caveats it prints for itself: the units are **KiB** while
heaptrack is **decimal MB** (4.9% apart, §21), and the `accounted / rss`
line is a **mid-run snapshot**, not the settled state (§11).

### 7.6 Traps, all of which have cost a run

1. **`MINIWORD_OPEN`, never argv** — argv is ignored and a blank document
   plateaus convincingly.
2. **Never `MINIWORD_SHOT`** while profiling — `process::exit(0)` skips
   heaptrack's atexit flush; the profile is empty.
3. **Export the variable**; `heaptrack … env VAR=x ./app` traces `env`.
4. **Wait for a plateau, not a clock** — heaptrack slows startup ~10x.
5. **Use the distinct corpus.** `big.md` is 98% duplicate lines; a
   caching change measured against it looks ~40% better than it is.
6. **`LC_ALL=C`** on anything parsing heaptrack output. Under a
   comma-decimal locale awk reads `9.57M` as `9` (§19). Fixed in the
   script; do not re-introduce it in an ad-hoc one-liner.
7. **Check a subset never exceeds its superset.** Cheapest self-check
   there is, and it caught the locale bug.

---

## 8. Risks, and what would make this NOT worth doing

### 8.1 The counter-principle

Servo, explicitly: *"Less memory usage isn't always better in browser
engines … there are many kinds of caches we can do to make browsing
faster, at the expense of increased memory."*

This plan does **not** reduce cache residency — it reduces the byte cost
of each cached entry, and step 4 *increases* effective sharing. But the
principle bites in two specific places:

- **Step 5c** replaces byte-slicing of `cluster.text` with flag lookups.
  If a flag is missing, the fallback is a `run.text` slice **plus** a
  pointer chase to the run, which is slower than today's inline `String`.
  The flag set in §3.2 must be complete before `text` is deleted.
- **Step 5** deletes `InlineItemMetrics`, which exists for incremental
  IFC relayout (`layout_tree.rs:243-250`, `window.rs:9275-9480`). Deriving
  those fields is O(1) each but adds indirection to the hot edit path.
  `layout/tests/contenteditable_e2e.rs::keystroke_cost_on_the_incremental_path`
  is the gate.

Raph Levien on word caches: the key hashing is *"nontrivial"*, it needs
locking, and it degrades on scripts without space-delimited words (Thai).
Our `per_item_shaped` key (`cache.rs:7593-7601`) already hashes item text
+ `layout_hash` + bidi level + script per group. Step 4 does not change
the key, only what a hit returns — but if step 4's measured win is small,
the reason is probably the hit rate, and *that* is Levien's problem, not
a byte problem.

### 8.2 Zed is a cautionary tale, not a model

Zed's arena post has **no numbers at all**; a GPUI maintainer says they
re-lay-out the whole app every frame and calls text-layout caching a
*"micro-optimization"* — with a user reporting **120 → 80 FPS** from
shape-cache misses (RESEARCH §4.7). Do not let "less retained state" drift
into "less caching".

### 8.3 The specific landmines in this codebase

| risk | where | mitigation |
|---|---|---|
| **The style/identity re-stamp** — sharing cache entries silently hands back the wrong colour and mis-attributes damage | `cache.rs:7606-7654` (fix `8ec9f387d`, **no test**) | **T2, shown red first.** Non-negotiable. |
| **printpdf is a published crate** pinned at 0.12.5 and compiled against our source | `dll/Cargo.toml:169`, `Cargo.toml:52-54`, `printpdf/src/html/bridge.rs:1031` | §5.1 freeze. A signature change needs a printpdf release chain. |
| **`#[repr(C, u8)]` is a WASM-lift fix**, not decoration | `cache.rs:4581-4586`, `:4453-4456` | Keep an explicit u8 tag at offset 0 on any replacement enum. |
| **A second stub definition** of `ShapedItem`/`UnifiedLayout` for the no-text-layout build | `layout/src/font_traits.rs:195`, `:222` | Change both; add that config to the local battery. |
| **`GraphemeClusterId` is FFI-frozen** | `api.json:13693`, `core/src/selection.rs:72` | `start_byte: u32` + `run.source_run: u32` reconstruct it exactly. Do not widen either. |
| **Vertical / tate-chu-yoko / ruby / shape-outside are effectively unpinned** | §2.1, T5/T6 | Write T5 and T6 before step 3. `azul-mock-vertical.ttf` already exists and no test loads it. |
| **`apply_text_orientation` clones every cluster** for vertical text | `cache.rs:8707-8742` | It returns `items` unchanged for horizontal (`:8712`), so it is not on the common path — but step 5 must not make the vertical path allocate more. |
| **57 dead tests look like coverage** | `layout/tests/text3/` | T0, blocking. |

### 8.4 What would make this NOT worth doing

State these honestly, because three of them are real.

1. **The floor is 78.5 MB and this refactor does not touch it.** Best
   case at 960 realistic lines is ~100 MB. If the actual requirement is
   "well under 100 MB", this is necessary but not sufficient, and the
   remaining work is on the *fixed* side: 15.6 MB of frame buffers (the
   GPU alternative is +67 MB worse), 14.0 MB of binary, 10.8 MB of GTK's
   345 shared-library mappings, 5.2 MB of startup CSS churn that has no
   defence either. **Decide the target number before starting.**

2. **~9.9 MB of the 48.09 MB has a backtrace owner but no object owner**
   (§14), and six hypotheses for it have been eliminated (§§12-16). If
   that memory is not inside the structures being shrunk, the allocator
   -side saving is ~36 MB, not ~46. The engine-accounted saving (~27 MB
   of ≤28.09) is not at risk; the difference between the two instruments
   is.

3. **Blink measured that field-shrinking does not matter** and got 82%
   from structure instead (RESEARCH §4.4). Steps 2 and 3 are
   field-shrinking. They are ordered early because they are cheap and
   because step 4 needs the run table — but **if step 3 measures under
   5 MB, that is the signal to stop shrinking fields and go straight to
   steps 4 and 5.**

4. **Effort.** `layout/src/text3/cache.rs` is 15 989 lines with 218 unit
   tests; `ShapedCluster`/`ShapedGlyph` have 60+ distinct read sites across
   `text3`, `solver3`, `window.rs` and `glyphs.rs`. Step 5 alone touches
   most of them. The prior research document scoped a similar change as
   *"high complexity — the destination, not the next step"* (RESEARCH
   §8 P6).

5. **The measure/layout duplication might be fixable more cheaply.**
   `measure_intrinsic_widths` (`cache.rs:6830`) exists to avoid the
   `BreakCursor::peek_next_unit` clone storm (see its doc comment at
   `:6813-6817`, and the 24%-CPU note). If the intrinsic pass could
   *avoid retaining* its shaping output rather than cache it, that is a
   much smaller change. **UNKNOWN whether it can** — the whole point of
   caching there is that `layout_flow` then hits it. Worth one experiment
   before step 4: disable the `shaped_items.insert` in
   `measure_intrinsic_widths` (`cache.rs:6914`) and measure both RSS and
   layout time. If RSS drops materially and time does not, most of this
   plan is unnecessary.

### 8.5 Things this plan deliberately does not propose

- Any allocator, arena, or `bumpalo` (§0.5).
- Interning `StyleProperties` by value beyond what commit `743eb5837`
  already did — but **do keep watching the count**: 419 distinct Arcs for
  32 937 glyphs is ~40x what a document has styles (§10), and T4/4 pins
  it. The direct cost is ~100 KB; the indirect cost is cache misses,
  because `ShapedItemsKey.style_hash` derives from them.
- Viewport-bounded retention (VS Code's model: do not retain off-screen
  layout at all). It would beat everything here, and it is a
  *different, larger* project that changes the engine's contract with the
  app. Note it as the next horizon, not as part of this.
- Fixing the three failing inline reftests.
- `NodeId`'s missing niche across the whole repo (§3.4).

---

## 9. Ledger — what is UNKNOWN

| question | status |
|---|---|
| Fraction of clusters that need the detail table on real prose | **UNKNOWN.** §3.5 assumes 2%. Latin measures 1:1 glyphs:clusters (§10), so 2% is a guess covering ligatures + combining marks. **Measure it before step 5d** — a counter in `memory_report` is a five-line change. |
| Split of `shaped_items_bytes` vs `per_item_shaped_bytes` today | **Measurable but unmeasured.** Both fields exist (`cache.rs:6157`, `:6161`) and are printed (`window.rs:3202`, `:3205`). Read them before deciding §4 Option B. |
| Whether the `measure_intrinsic_widths` cache insert can simply be dropped | **UNKNOWN.** §8.4/5 — one experiment settles it and could shrink this whole plan. |
| Whether step 3's saving justifies its cost | **UNKNOWN by design.** BlinkOn says no, our struct arithmetic says yes. The step is ordered so that its measurement decides. |
| Gecko's compact-path hit rate | **NOT VERIFIED and does not exist in public writing** (RESEARCH §9). Our detail-table incidence is ours to measure. |
| Whether the ~9.9 MB residual lives in these structures | **UNKNOWN.** Six hypotheses eliminated; the search is deliberately closed (§16). It will resolve itself as a measurement after step 5, one way or the other. |
| Effect on non-Latin scripts | **UNMEASURED.** Every figure here is a Latin 1:1 document. Arabic and Devanagari have many-to-many clusters and will lean on the detail table; CJK has more distinct glyphs and fewer runs. |
| Effect on documents with images or tables | **Out of scope for the map that produced these numbers** (§7u). |

---

*No optimisation in this document has been implemented. Check `git log`
and `git status` before starting — the tree moved twice while this was
written (§0.4).*

## 10. NIGHT-2 STATUS ADDENDUM (2026-08-11 06:25, written in-repo because §9's unknowns moved)

Implemented and PUSHED through `c04a69b4d` (this section supersedes the
"nothing implemented" footer above for §2/§3/§4):

- §2 gates: T1 roundtrip (11 props), T2 identity + NC knob, dense
  equivalence gates ×7 — all live in `layout/tests/`.
- §3.2 types: `dense.rs` (ClusterCompact 16 B, DenseRun + direction,
  LineRecord + source_index, detail tables) + dense twins for ALL THREE
  walkers (positions d61be0d07, simple runs + PDF runs 681bcd5de),
  each reference-exact over the gate corpus, NCs seen red.
- §4 partial: ShapedCluster::text DELETED (c04a69b4d) — clusters slice
  a shared Arc of the LOGICAL ITEM text; walk 323 → 310 B/cluster.

FINDINGS the plan's next implementer needs:

1. `start_byte_in_run` is LOGICAL-ITEM-relative, not run-relative
   (override segments carry their run offset in `item_index` — see the
   coalescing re-attribution in cache.rs). DenseText::from_unified's
   `content.get(source_run)` text mapping is therefore WRONG for
   override-segmented runs (invisible on all current corpora — no
   consumer feeds overrides). Fix by keying run text off the ITEM, or
   folding item_index into the dense builder, BEFORE flipping any
   production consumer that can see IME/spelling overrides.
2. Glyph metrics ARE cluster-uniform (one font per shape_text_correctly
   call; fallback splits text BEFORE shaping) — the slice-1 note saying
   otherwise was wrong. But a glyph→cluster metrics hoist is a WASH for
   1-glyph clusters (32 B moves, net 0); the metrics economics only
   materialise at RUN level, i.e. in the dense flip itself. Do not do a
   standalone metrics slice; CombinedBlock uniformity still unverified.
3. The flip order that follows from what exists: (a) build DenseText in
   `layout_cached_with_dl` next to the sparse layout behind a flag,
   (b) flip `get_glyph_runs_simple`'s call site (layout_tree.rs:286/314)
   to the dense twin + A/B e2e over the corpus, (c) flip
   get_glyph_positions consumers, (d) PDF last (printpdf contract), then
   (e) stop retaining the positioned-item text path warm and re-measure
   with scripts/rss-baseline.sh AT PLATEAU (the walk's B/cluster cannot
   see malloc-overhead wins; allocator truth rules — batch ruling says
   this is the ONE final measurement).

### §10.4 addendum (2026-08-11 11:05, after flip (a) dbdbdebd7)

Steps (b)/(c) of the §10.3 flip order are NO-OPS: `get_glyph_positions`
has zero production consumers (reference/tests only) and the PDF export
walks the DISPLAY LIST, not `get_glyph_runs_pdf` (the twin's value was
proving 3c's reconstruction, already banked). The campaign therefore
goes from flip (a) directly to (d), whose honest shape is: make
`DenseText` the STORED form of `CachedInlineLayout` and serve every
current `UnifiedLayout` reader from it — direct `layout.items` readers
are ~21 sites in 5 files (window.rs 7, layout_tree.rs 6,
display_list.rs 5, paged_layout.rs 2, fc.rs 1) plus the accessor-level
readers behind `get_inline_layout_for_node` (selection, caret, edit,
hit-test). Retire the sparse retention LAST, after every reader has a
dense view; measure at plateau before and after (e).

### §10.4b THE RETIREMENT PLATEAU (2026-08-12 02:22, f32a3b23b)

uniq-960, windowed, same rig, AZ_DENSE_TEXT=1 (sentinel retirement
active): **RSS 127.1 MB (VmRSS 127148 kB), [heap] 65.2 MB** — vs 136.0
/ 73.6 flag-off pre-retirement and 147.9 / 90.6 rig baseline. The
retained sparse shed ~11 MB (prediction was ~8; Vec/SmallVec overhead
above the raw 192 B x 31k). RSS tracked heap ~1:1 this time (-8.9 for
-8.4) — the freed slabs returned to the OS, unlike leg 1's 0.7 factor.
Campaign to date: heap -28.0%, RSS -14.1%. Remaining: d7 (delete the
monolithic stage-3 shaped_items map — 6.1 MB fat-era, duplicates
per_item_shaped; edits always missed it and post-R1/R2 resize rarely
re-lays-out — then compact per-item entries) + the AZ_DENSE_TEXT
default flip.

### §11 THE SHED REPORT (2026-08-12, campaign complete)

All numbers: uniq-960 (960-line markdown), windowed KDE Wayland, CPU
backend, the same rss-baseline rig throughout.

| stage | RSS MB | heap MB | landed |
|---|---|---|---|
| rig baseline (RSS_MAP era) | 147.9 | 90.6 | — |
| slice1 + warm-split + flags + 3c + shared Arcs | 136.0 | 73.6 | dbdbdebd7..c04a69b4d |
| + dense arrays, dual retention (transitional) | ~138 | ~76.2 | d1-d6g |
| THE RETIREMENT (sentinel, dense stored form) | 127.1 | 65.2 | f32a3b23b |
| d7: cache dedup + segmented compaction + DEFAULT ON | **121.5** | **62.3** | 0a5c69230 + d7b |

**TOTAL: heap −28.3 MB (−31.2%), RSS −26.4 MB (−17.9%).**

The structural story: one 960-line document's shaped text was retained
THREE times (~200 B/cluster sparse in the layout cache, again in the
monolithic stage-3 shaping cache, again in the per-item shaping
cache). Now: the layout cache stores 16 B/cluster dense arrays
(sparse materializes transiently for rare geometry readers), the
monolithic cache is deleted, and the per-item cache stores segmented
compact entries (1.2 MiB total, 174 atoms / 706 segments). Allocator
health at plateau: 6-7% arena slack, 0 releasable — the heap number
is live data, not fragmentation.

Against the field (same corpus, §10.5 below): miniword 121.5 MB now
sits −34% under Blitz (211), −41% under LO Writer (206), 2.5-9.6x
under the Chromium/Electron/Firefox class; the only lighter apps are
GTK buffer widgets that do no styled layout (gedit 93, bare TextView
49). Binary: libazul 34 MB stripped ~ Blitz 39 MB.

Deferred follow-ups (size-only, correctness intact): item_base
run-split degeneracy on unpopulated-item_index paths (~25 KB/IFC);
glyph_runs paint cache is the remaining per-glyph retention
(~5-6 MB, predates the campaign; next big target with LayoutFontMetrics
sharing).

### §11b post-campaign (#25, 2026-08-12): paint-glyph retention + the RSS anatomy

The report walk was BLIND to every paint-side glyph retention (never
counted `glyph_runs`, and the cached display list was a flat 2048-byte
guess). First honest itemization at plateau (windowed uniq-960,
AZ_PROFILE=memory build, RSS 129.2 profiled):

    glyph_runs        1188 KiB   814 runs / 28,206 instances / 43 B/inst
                                 (20 B GlyphInstance + Vec-growth slack)
    cached_display    2855 KiB   the SAME 28,206 instances AGAIN as
                                 offset copies inside DL Text items
    warm.inline       6445 KiB   (the campaign's 210 B/cluster)

Fixes: CompactGlyphRun stores (u32,f32) pairs at 8 B/glyph, y/size
hoisted per run, full-instance exception table for deviants, bit-exact
roundtrip gated under verify; screen DLs stop emitting TextLayout
(printpdf/paged metadata; both screen renderers no-op it) — gated on
`fragmentation_context.is_some()`, the existing screen-vs-paged
discriminator. The DL's Text-item copies remain (printpdf freezes the
variant's `Vec<GlyphInstance>` field); folding them onto the compact
runs needs the printpdf release chain — ledgered, not attempted.

ANON BLOCKS ATTRIBUTED (2026-08-12, window-size scaling experiment
800x600 vs 1600x1000 — the giant scales at ~2x W*H*4 while a 1192 kB
block is size-invariant): they are the CPU present path's TWO
full-window pixmaps — `CpuBackend.last_frame` (the round-3 retained
blit source, headless/mod.rs) + the compositor's per-layer pixbuf
(cpurender/compositor.rs:684) — plus the glyph cache (the invariant
block). With the 2-slot shm pool (CpuFallbackState, = the azul-fb
memfd, exactly 2 frames) the presentation total is FOUR window frames.
All four are design-carried: blit source, compose target, and
protocol-safe double buffering. A possible -1 frame (compose directly
into last_frame when single-layer) would touch the present path the
user just verified live as smooth — parked as a design option, not
attempted.

LANDED 7a40acb73, full battery green (8255+8321 incl corpus, doc,
reftest 47/52 baseline, dll 1780). AFTER on the same rig:
glyph_runs 1188 -> 372 KiB (13 B/inst amortized), cached_display
2855 -> 1703 KiB (3492 items x 232 B/slot; dropping ~800 TextLayouts
also pulled the item + parallel vecs under a power-of-two capacity
boundary), plateau **120.4 RSS / 59.4 heap** clean (was 121.5/62.3)
= heap **-34.4%** from the campaign's 90.6 baseline. Anon-mmap
breakdown (the RSS-heap gap): 25.5 MB in 32 maps, top blocks 9.5 MB +
8.0 MB + 3.5 MB — pixmap/atlas class (nearly fully resident, too hot
for stacks); attributing THOSE is the next profiling run
(AZ_PROFILE=heap w/ probe feature names owners).

RSS − heap anatomy at the same plateau (smaps categories, the "60 MB"
question): anon mmaps 21.5 MB (glibc mmaps allocations >128 KB
DIRECTLY — they are malloc bytes that [heap] does not show), binary
code/rodata 16.9 MB resident, azul-fb shm frame buffers 4.9 MB
(= W×H×4×2 buffers, the irreducible shm-client floor; 1920×1080
double-buffered would be 16.6 MB by construction), wayland-cursor
theme 1.1 MB, shared libs ~6 MB, mmap'd fonts ~1.7 MB. KDE system
monitor's "113 MB" ≈ PSS (122 MB profiled here), consistent.

### §11c #25b (f47dff612): the item-index dual mode — the sleeper win

The "~25 KB/IFC size-only" ledger item was mis-scoped by two orders of
magnitude: item_index is constant per logical item while start_byte
advances WITHIN it, so the d6h linear-only item_base split a fresh
DenseRun (header + 32 B font_metrics copy) on EVERY cluster inside
every multi-cluster word — ~28k degenerate runs on uniq-960.
DenseRun.item_linear (builder tracks both reconstruction models, close
prefers linear, expander branches; NC seen red) coalesces them:

    warm.inline   6445 -> 2501 KiB   (the runs-header waste, gone)
    plateau       120.4/59.4 -> **109.0 RSS / 51.5 heap**

Campaign totals from the rig baseline 147.9/90.6:
**RSS -26.3%, heap -43.2%.** Full battery green (8256+8322 incl
corpus, doc 186, reftest 47/52 baseline, dll 1780).

mimalloc A/B (user idea): RUN 2026-08-12, CLOSED — glibc WINS.
Same corpus/protocol, `-F azul-dll/allocator_mimalloc` (existing
feature, mimalloc confirmed active via /proc/maps): **127.7 MB RSS
vs 121.5 glibc** (+6.2 MB, ~5% worse). Expected in hindsight: the
campaign moved the allocation profile to few large flat arrays +
segmented caches — glibc's best regime (hence the 6-7% slack / 0
releasable above) — while mimalloc pays fixed per-segment arena
overhead. Its tiny `[heap]` figure (760 kB) is an artifact of mmap
arenas, not a win; RSS is the honest total. glibc stays the default;
the `allocator_mimalloc`/`allocator_jemalloc` features remain for
hosts with small-object-churn profiles.

### §10.5 external yardstick (2026-08-11, measured on this machine)

LibreOffice Writer 24.x (deb install, first profile run, windowed on
KDE Wayland, 50 s settle) with the SAME uniq-960 corpus loaded:
**soffice.bin 206 MB RSS**. miniword at the d6d flag-on plateau:
**136.0 MB RSS / 73.6 MB heap** → 0.66x LO mid-campaign, with the
dense+sparse dual retention still in (transitional +2.6 MB) and the
shaped-text kill (§7k) not yet landed. Method note: capture RSS to a
file BEFORE any pkill, and never `pkill -f` with a pattern that appears
in the measuring shell's own command line (it self-kills, exit 144).

Full 2026-08-11 sweep, same corpus, windowed, 30-50 s settle,
process-TREE RSS (browser-class apps got a styled-HTML render of the
corpus, one <p>/line + page CSS; editors got the raw txt):

| tier | app | MB RSS |
|---|---|---|
| buffer editors (no styled layout, no pagination) | PyGTK3 TextView (minimal) | 49 |
| | gedit | 93 |
| | PyQt5 QPlainTextEdit (minimal) | 160 |
| | kwrite / kate | 196 / 197 |
| document renderers (styled layout + pagination) | **miniword (azul, CPU)** | **136** |
| | LibreOffice Writer | 206 |
| web engines (full doc stack) | Blitz browser (Stylo+Parley+vello_cpu, 1 proc) | 211 |
| | Servo v0.4 (1 proc) | 289 |
| | Electron v43 bare BrowserWindow (4 proc) | 324 |
| | Chromium (4 proc) | 426 |
| | Firefox (11 proc, fission; 3 proc on txt = 678) | 1166 |

Readings: miniword undercuts every Qt app on the box INCLUDING a
minimal QPlainTextEdit; only GTK3 buffer widgets doing no document
work are lighter; every web-tech route to the same UI is 1.55-9x.
Python rows carry ~15-25 MB interpreter. Electron row is an EMPTY
shell app — real Electron apps add their bundle on top. Blitz was
built from source (no prebuilt releases; needed rustls swap in
blitz-net + fontconfig/ssl dev headers): the closest peer stack
(Rust, CPU render) and STILL 1.55x azul's RSS. WPS skipped (system
install). Binary sizes (stripped, self-contained): libazul 34 MB ~
blitz 39 MB < servo 131 < ff 297 < electron 313 < LO 320 < chromium
359; system editors 0-7 MB riding 100+ MB shared toolkits.
