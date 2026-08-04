# Design: block break tokens + page loop (K30–K36) — true fragmentation for azul

2026-08-04. Design only (no code changed). Successor plan to the post-hoc
display-list slicer: adopt Blink LayoutNG's **break-token** architecture so
per-page output is *generated* by layout instead of *cut out of* an
infinite-canvas layout after the fact. Grounded in the fragmentation research
(`../pdf2html/research-fragmentation-web.md`), the critique ledger
(`../pdf2html/AZUL-STILL-TODO.md` §K), and a fresh audit of the actual seams
(file:line refs throughout; see the appendix inventory).

> **The one-line verdict from the research:** the W3C model says fragmentation
> is *geometry, not tree* — one box → N fragments, the box tree never mutates.
> True fragmentation is therefore not a departure from azul's immutable-DOM
> philosophy; it is its layout-level completion.

> **The two headline de-risks:** azul already owns the hard half — text3's
> `BreakCursor` (`layout/src/text3/cache.rs:11460`) *is* an inline break token
> (NG's `InlineBreakToken` equivalent), and `layout_flow`
> (`layout/src/text3/cache.rs:6382`) already runs a fragmentainer loop over a
> `flow_chain: &[LayoutFragment]` and returns `FlowLayout.remaining_items` —
> the "didn't fit, resume here" output. What's missing is only the **block**
> generalization and a **page-loop driver**.

---

## 1. Why (and why now)

### 1.1 What the slicer can never get right

Today's paged pipeline lays the document out on an infinite canvas, computes
break Ys (`compute_page_breaks*`, `layout/src/solver3/page_breaks.rs`), then
slices the one display list into pages
(`paginate_display_list_with_breaks`, `layout/src/solver3/display_list.rs`).
The 2026-08-04 wave patched the worst slicing artifacts — E17 re-derives the
clip/stacking marker structure per page, E18 places repeated theads x-aware,
E19 per-page footer heights, E21 reports monolith tears — but four failures
are *structural* (research §5.3) and cannot be patched post-hoc:

1. **Unforced break placement.** `avoid` / `widows` / `orphans` decisions are
   made by *moving a Y after the fact* (`snap_break_up`), bounded by
   `max_push_distance`. Correct placement is a layout-time decision with
   knowledge of what re-laying the flow would produce (NG's early-break +
   one-relayout rule, §4.6 below).
2. **Margin truncation at breaks.** css-break-3: margins adjoining an
   unforced break are truncated. The canvas layout has already collapsed and
   materialized them; the slicer can only cut through the result.
3. **Per-page exclusion geometry.** A float near a page boundary must occupy
   wrap space *per fragmentainer* — next page's lines wrap around the float's
   *continuation fragment*, with no memory of its old-page geometry
   (css-break-3 parallel flows; research §4). Sliced infinite-canvas text has
   the *wrong line widths* near every boundary a float crosses. This is the
   correctness gap behind H22 (drag image → text rewraps → flow crosses
   pages live).
4. **Page-scoped content.** `box-decoration-break: clone`, Class C breaks,
   page-counter-dependent content — all need the box to *know* it broke.

Monoliths: NG lets a taller-than-fragmentainer box **overflow** (print slices
graphically at paint time); our slicer *cuts* it. E21's `MonolithWarning` at
least reports it now; tokens adopt the NG overflow rule outright.

### 1.2 The fossil warning — why the old in-solver attempt failed

`layout/src/solver3/paged_layout.rs:1-11` still documents the previous
architecture: *"`page_index` is assigned to nodes DURING layout based on Y
position"*. That attempt — like Gecko's continuation frames (the documented
pain path, research §2.2) — threaded **mutable pagination state through
layout**. The bug farm it produced is why the slicer exists. The token design
is immune *by shape*: layout stays a pure function; fragments and tokens are
**value-type outputs**; nothing about pagination is ever written into the
node tree, the layout tree, or any cache during the pass.

```
(node, constraint_space { remaining_extent }, break_token?) -> (fragment, break_token?)
```

This is the same input→output philosophy as the overlay/changeset editing
model: the DOM (and here, the layout tree) is never mutated; deltas are
values.

### 1.3 What already exists (audited inventory)

| Piece | Where | Status vs design |
|---|---|---|
| Inline break token | `BreakCursor` — text3/cache.rs:11460 (`items`, `next_item_index`, `partial_remainder`, word-break knobs) | **is** the NG InlineBreakToken; needs an *owned, Eq* snapshot form (§4.2) |
| Fragmentainer loop (inline) | `layout_flow(content, …, flow_chain: &[LayoutFragment])` → `FlowLayout { fragment_layouts, remaining_items }` — text3/cache.rs:6382/5885 | the multi-fragment driver exists for IFCs; page loop generalizes it to blocks |
| Block layout entry | `layout_bfc(ctx, tree, text_cache, node_index, constraints, float_cache) -> BfcLayoutResult` — solver3/fc.rs:1060 | gains fragmentainer awareness + resume (§4.4) |
| Constraint input | `LayoutConstraints` — fc.rs:131 (no remaining extent); `BfcState { pen, floats, margins }` | gains one `Option<FragmentainerSpace>` field (§4.3) |
| Media selector | `FragmentationContext { Continuous, Paged }` — core/src/paged.rs:24 | today metadata-only (slicing is post-hoc, its own doc comment says so); becomes the switch that arms the page loop |
| Break pass | `compute_page_breaks*` + `BreakPolicy` + E21 `MonolithWarning` — solver3/page_breaks.rs | policies + appeal semantics migrate into layout-time early-breaks (§4.6); Y-pass retained for the slicer path until retirement |
| Slicer | `paginate_display_list_with_breaks` + E17 marker re-derivation + E18 thead bands + E19 sequence heights — solver3/display_list.rs | **kept as the differential-testing oracle** and the screen path until parity (§6) |
| Per-page setup | `PageSequence` / `PageSetup::content_height()` — solver3/pagination.rs:643 | feeds per-page fragmentainer extents directly (already per-page in the break pass) |
| Incremental session | `PaginationSession` / `BreaksDelta` (bit-exact prefix) — solver3/paged_layout.rs | superseded by token convergence (§4.7), which is strictly stronger |
| Sectioned stepping stone | `compute_sectioned_pagination` — paged_layout.rs:985 (spine-cut per width section, clones DOM) | superseded; real re-layout per section but block-granular cut |
| Editor bridge | `pagination_to_dom_breaks`, `pagination_dirty_from`, `NodeType::PageBreak` (A1/B6/A3) | unchanged v1; K *removes the need* for app-side break nodes later (§7 phase 3) |

---

## 2. Goals / non-goals

**Goals (in phase order):**
- K30: block break tokens + a page-loop driver; per-page display lists
  *generated*, never cut. PDF path first, behind a runtime engine switch.
- K31: margin truncation at unforced breaks; Class A/B break points; forced
  break propagation from first/last children.
- K32: per-page exclusion spaces (floats + Word-style anchored objects) —
  the piece that makes H22 *correct*, not just fast.
- K33: widows/orphans/avoid via recorded early-break candidates + at most
  **one** relayout per flow (NG's sanctioned cost).
- K34: token-convergence incremental repagination (the live-editing engine;
  Blink doesn't ship this — azul can beat the browser here).

**Non-goals (v1):**
- No retained fragment *tree*. Per-page output = display list + per-page
  geometry, exactly what `DomLayoutResult` holds today (research §5.2 says
  this suffices to ship).
- No `@page` CSS parsing (the app drives `PageSequence` programmatically).
- Screen path untouched in phase 1 — `Continuous` layout, the interactive
  editor's break-node model (A/B sections), and the slicer stay as-is.
- OOF fragmentation parity with NG (fragmentainer-parented absolutes) —
  documented divergence v1, revisit in K32.

---

## 3. Core contract

One new pure function per formatting context, uniform shape:

```rust
/// The fragmentainer-facing layout contract (NG: Node × ConstraintSpace ×
/// BreakToken → Fragment × BreakToken). PURE: no tree mutation, no cache
/// writes keyed by page; all pagination state lives in the token VALUES.
fn layout_fragment(
    ctx: &mut LayoutContext,          // fonts/caches — same as today
    tree: &LayoutTree,                // read-only node/style access
    node: usize,                      // layout-tree index
    space: &FragmentainerSpace,       // §4.3
    incoming: Option<&BreakToken>,    // None = first fragment of this box
) -> Result<FragmentResult>;

pub struct FragmentResult {
    /// Geometry + display items for THIS fragmentainer only, in
    /// fragmentainer-local coordinates (y = 0 at the fragmentainer top).
    pub fragment: PageFragmentOutput,
    /// None = this box (and all its content) is FINISHED.
    pub outgoing: Option<BreakToken>,
}
```

The **page loop** is the only new driver (§4.5): feed page N−1's outgoing
token into page N until `None`.

---

## 4. Design detail

### 4.1 Token types — owned, `Eq` value types

```rust
// layout/src/solver3/break_token.rs (new)

#[derive(Debug, Clone, PartialEq)]
pub enum BreakToken {
    Block(BlockBreakToken),
    Inline(InlineBreakToken),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockBreakToken {
    /// The box this token resumes (layout-tree index + its NodeId for
    /// diagnostics/remap; the token is regenerated per relayout so ids are
    /// same-generation by construction).
    pub node: usize,
    /// Block-size of this box already consumed by previous fragmentainers
    /// (border-box progression for `box-decoration-break: slice`, and the
    /// resume offset for monolith overflow).
    pub consumed_block_size: f32,
    /// Children in DOCUMENT ORDER. NG invariant (their commit history:
    /// violating it caused infinite loops): every sibling BEFORE the first
    /// entry is FINISHED; entries are the unfinished tail.
    pub children: Vec<ChildBreakEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChildBreakEntry {
    /// Child started in an earlier fragmentainer; resume it with this token.
    ResumeIn { child: usize, token: Box<BreakToken> },
    /// Child not yet started (a break landed before it — including
    /// `break-before` forced entries).
    BreakBefore { child: usize },
}

/// Owned snapshot of text3's `BreakCursor` (which borrows `&'a [ShapedItem]`
/// and therefore cannot itself be stored/compared across passes).
#[derive(Debug, Clone, PartialEq)]
pub struct InlineBreakToken {
    /// Resume index into the IFC's shaped-item sequence.
    pub next_item_index: usize,
    /// The hyphenation remainder (`BreakCursor.partial_remainder`), owned.
    pub partial_remainder: Vec<ShapedItem>,
}
```

**Equality semantics.** K34 needs cheap, *reliable* `==` on tokens.
`ShapedItem` carries `f32` geometry: equality must be **bit-exact**
(`f32::to_bits`), the same contract `PaginationSession`'s unchanged-prefix
already uses (precedent: paged_layout.rs, B7). Two practical notes:
- Derive `PartialEq` structurally where the field types already compare
  bit-honestly; where `ShapedItem` compares floats via `==`, that is
  IEEE-equality — fine for convergence (identical passes produce identical
  bits; the -0.0/NaN corner cases cannot arise from a deterministic
  re-layout of identical input, and a false *inequality* only costs one
  extra page of relayout — safe direction).
- For the convergence hot path, cache a 64-bit FNV/hash of the token beside
  it (`token_fingerprint()`); compare hash first, full struct second. Never
  hash-only (collision → wrongly stopping repagination).

**Bridge:** `InlineBreakToken::from_cursor(&BreakCursor)` /
`BreakCursor::resume(items, &InlineBreakToken)` — pure conversions next to
`BreakCursor` in text3. `layout_flow`'s `remaining_items` output maps 1:1
onto a token whose `next_item_index` points past the consumed prefix.

### 4.2 What is deliberately NOT in the token

- **No geometry of previous pages** (only `consumed_block_size` scalars).
  Exclusion geometry is *reconstructed* per fragmentainer (§4.6/K32) — the
  research is explicit that carrying old-page wrap geometry forward is the
  Regions-killing circularity.
- **No DOM/overlay references.** Tokens are layout-tree-indexed and die with
  the layout generation. Repagination across an EDIT always starts from the
  first dirty page (whose incoming token predates the edit and is therefore
  still valid — §4.7).

### 4.3 Constraint plumbing — one `Option` field

`LayoutConstraints` (fc.rs:131) gains:

```rust
pub struct FragmentainerSpace {
    /// Block-extent remaining in the CURRENT fragmentainer, measured from
    /// the current pen. Layout code compares child block-sizes against this.
    pub remaining_block_extent: f32,
    /// Extent of a FRESH fragmentainer (for "won't fit here, will it fit on
    /// the next page at all?" monolith classification and for K33 appeals).
    pub next_fragmentainer_extent: f32,
    /// True while laying the first fragmentainer of the flow (Class C /
    /// margin rules differ at the very start).
    pub is_first: bool,
}

pub struct LayoutConstraints<'a> {
    // ... existing fields unchanged ...
    /// None = continuous media (screen): today's behavior, bit-for-bit.
    pub fragmentainer: Option<FragmentainerSpace>,
}
```

`None` everywhere on the screen path — the continuous pipeline cannot be
perturbed by construction (this is the K36 "keep the slicer" guard applied
to the *solver* too). `FragmentationContext::Paged` (core/paged.rs) is what
arms it, finally making that enum operational instead of advisory.

### 4.4 `layout_bfc` fragment-aware (the K30 core)

The child loop in `layout_bfc` (fc.rs:1060) gains two behaviors, both
no-ops when `constraints.fragmentainer.is_none()`:

**Breaking (producing a token):**
1. Before placing child *i*: if the child's block-size (from its own layout
   at this width) exceeds `remaining_block_extent − pen`:
   - child is *breakable* (block container / IFC, no `break-inside: avoid`
     in force at this appeal level): recurse with a reduced
     `FragmentainerSpace`; the child returns `(fragment, Some(child_token))`
     → place the fragment, emit `ResumeIn { child, token }`, **stop
     consuming siblings**, return
     `Some(BlockBreakToken { node, consumed_block_size, children })`.
   - child is a **monolith** (replaced content, `avoid` at max appeal, line
     box): if it fits a *fresh* fragmentainer → `BreakBefore { child }` and
     stop; if it can never fit (`> next_fragmentainer_extent`) → place it
     OVERFLOWING the fragmentainer (NG monolith rule; paint clips at the
     page box) + push an E21 `MonolithWarning` — never tear, never loop.
2. IFC children: run `layout_flow` with the remaining extent as the
   fragment's height constraint; non-empty `remaining_items` →
   `ResumeIn { child, token: Inline(...) }`.

**Resuming (consuming a token):**
- `incoming: Some(Block(tok))` → skip all children before the first entry
  (the NG *finished-siblings invariant*; assert it in debug), seed
  `BfcState.pen` at the fragmentainer top (0), `BfcState.margins` reset (a
  fragmentainer top is a margin-truncation boundary, §K31), resume entry
  children (`ResumeIn` → recurse with their token; `BreakBefore` → lay out
  from scratch), then continue with the remaining siblings normally.
- The box's own top border/padding are NOT re-emitted when
  `consumed_block_size > 0` (`box-decoration-break: slice` default);
  `clone` support later reads the same scalar.

`BfcLayoutResult` (fc.rs:88) carries the optional outgoing token up:

```rust
pub(crate) struct BfcLayoutResult {
    pub output: LayoutOutput,
    pub escaped_top_margin: Option<f32>,
    pub escaped_bottom_margin: Option<f32>,
    /// Some = this BFC ran out of fragmentainer space; resume with this.
    pub outgoing_token: Option<BreakToken>,   // NEW
}
```

Taffy-bridged contexts (flex/grid/table) are **monolithic in v1**: they
either fit the remaining extent, move whole to the next page
(`BreakBefore`), or overflow with a warning. (Row-level table fragmentation
via tokens is a K31+ extension; today's `TableHeaderTracker` thead
repetition moves into the page loop, which *knows* a table resumed on this
page — replacing the Y-heuristic `straddling_tables_for_page`.)

### 4.5 The page loop (new driver, `paged_layout.rs`)

```rust
pub enum PaginationEngine { Slicer, Tokens }   // runtime switch, NOT a cargo feature

pub fn layout_document_tokenized(
    /* same inputs as layout_document_paged_with_config */
    sequence: &PageSequence,
    only_page: Option<usize>,       // lazy single-page parity with the slicer
) -> Result<TokenizedPagination> {
    let mut incoming: Option<BreakToken> = None;
    let mut pages = Vec::new();
    let mut tokens = Vec::new();     // per-page (incoming, outgoing) for K34
    for page_idx in 0.. {
        let setup = sequence.setup_for_page(page_idx);
        let space = FragmentainerSpace {
            remaining_block_extent: setup.content_height(),
            next_fragmentainer_extent: sequence.setup_for_page(page_idx + 1).content_height(),
            is_first: page_idx == 0 && incoming.is_none(),
        };
        let result = layout_fragment(ctx, tree, root, &space, incoming.as_ref())?;
        // PROGRESS GUARD (the NG infinite-loop class, made structurally
        // impossible): the outgoing token must differ from the incoming one.
        if result.outgoing.as_ref() == incoming.as_ref() && result.outgoing.is_some() {
            warnings.push(MonolithWarning { /* no-progress */ });
            break;
        }
        pages.push(result.fragment);          // per-page DL, generated not cut
        tokens.push((incoming.clone(), result.outgoing.clone()));
        incoming = result.outgoing;
        if incoming.is_none() { break; }
    }
    Ok(TokenizedPagination { pages, tokens, warnings })
}
```

Per-page output feeds the exact consumers the slicer feeds today (printpdf
page loop, `paginate_single_page` thumbnails): the interface stays
`Vec<DisplayList>` + per-page node geometry. Header/footer/margin-box
decoration continues to come from `solver3::pagination` — decoration is
orthogonal to fragmentation and already per-page (E19).

### 4.6 K31–K33 on top of the same shape

- **K31 margins:** `MarginCollapseContext` resets at each fragmentainer top;
  a margin adjoining an unforced break is truncated to zero (css-break-3
  §5.2). Class A (between siblings) and Class B (between last child and
  parent end) break points fall out of the child loop; forced
  `break-before/after` on a first/last child propagates to the parent's
  boundary (today's `ForcedBreak` collection keeps working — forced breaks
  become `BreakBefore` entries at the right ancestor).
- **K32 exclusions:** `FloatingContext` becomes per-(BFC, fragmentainer):
  seeded at each page top from (i) float-continuation tokens (a broken
  float's token carries its remaining block-size → an exclusion band at the
  next fragmentainer's top, at the float's inline position) and (ii)
  objects anchored to this page (Word model: an anchored object belongs to
  its anchor paragraph's page; if the paragraph moves, both pages
  repaginate). Never copy wrap geometry across pages. This is what makes
  line widths near boundaries correct — the slicer's unfixable case.
- **K33 early breaks:** during the child loop, record
  `EarlyBreak { position, appeal }` candidates (appeal ordering:
  perfect > violates-avoid > violates-orphans/widows > slice-monolith —
  today's `BreakPolicy` knobs map onto which appeals are acceptable). When
  space runs out at a worse appeal than the best recorded candidate, abort
  and re-run the flow ONCE targeting it. Hard bound: a
  `debug_assert!(relayout_count <= 1)` per flow — NG's sanctioned cost.
  E21's `MonolithWarning` report moves from the Y-pass to this layer
  unchanged in shape.

### 4.7 K34 — token convergence (the live-editing engine)

Tokens are value types ⇒ **if the token entering page N is unchanged, pages
≥ N are unchanged** (research: Blink's architecture permits this but its
cache bypasses break-token'd nodes; it does not ship this).

```
on edit/drag:
  dirty_y   = pagination_dirty_from (B6 chokepoint, already fed by all edits)
  first_dirty_page = page_of_y(dirty_y)
  incoming  = cached tokens[first_dirty_page].incoming     // predates the edit; valid
  for page in first_dirty_page.. :
      (fragment, outgoing) = layout_fragment(..., incoming)
      if outgoing == cached tokens[page].outgoing:         // CONVERGED
          splice fragments; DONE — pages > page reused verbatim
      incoming = outgoing
```

Typing converges in ≤ 2 pages; a dragged image converges when the flow
re-synchronizes. Visible pages synchronously, remainder on idle (the
VirtualView window from D15 mounts pages; nothing about that model
changes — pages just come from the token cache instead of the estimator).
This subsumes B7's `BreaksDelta` with a strictly stronger layout-level
signal; `PaginationSession` keeps its API and swaps its internals.

---

## 5. What this retires (and when)

| Retired | By | When |
|---|---|---|
| E17 marker re-derivation | pages are generated inside the open structure — markers never cut | when tokens own a path (PDF first); slicer keeps its copy until deleted |
| E18 thead x-bands / `TableHeaderTracker` Y-heuristics | the page loop *knows* which table resumed (its token) — inject thead at resume | K31+ (tables) |
| `compute_page_breaks*` Y-pass + `snap_break_up` | breaks are placed during layout (K33 appeals) | slicer retirement |
| `compute_sectioned_pagination` spine-cut | per-page extents come from `PageSequence` directly in the loop | K30 lands |
| App-side break-node insertion (A1 consumers) | pages become layout outputs; `NodeType::PageBreak` remains as the *forced-break* marker only | phase 3, app opt-in (K35: keep that layer thin meanwhile) |
| `PaginationSession` bit-exact prefix | token convergence | K34 |

The A/B-section model (break-Y→DOM-path, estimation session) **remains the
correct v1 for the interactive editor** until phase 3 — typing-driven
pagination with break nodes ships today without K (K35).

---

## 6. De-risking & test plan (K36, concretized)

**Why this won't repeat the fossil:** pure function, value outputs — the
failure mode ("mutable pagination state threaded through layout") is
structurally absent. Concrete guards, in order:

1. **Keep the slicer. Differential-test against it.** Runtime
   `PaginationEngine::{Slicer, Tokens}` (an enum, deliberately NOT a cargo
   feature: both engines callable in one binary, same inputs) + a harness
   that runs BOTH on the corpus subset where slicing is *provably correct*
   (single column, no floats near boundaries, no avoid-constraints, no
   sequences) and asserts per-page display lists are `is_visually_equal`
   item-for-item. Corpus: `doc/working/*.xht` + the existing slicer unit
   fixtures. Where the slicer is known-wrong (float at boundary), golden
   token-page snapshots + one visual review each.
2. **Property-test tokens in isolation** (fast harness, no engine round
   trips — the "distill to unit test first" rule):
   - *Resume law:* `layout(content, token_at_break(Y))` ≡ layout of the
     remaining content in a fresh fragmentainer (for avoid-free content).
   - *Conservation:* Σ fragment block-sizes = unfragmented block-size
     (avoid-free, margin-truncation-aware).
   - *Progress:* `outgoing != incoming` for every `Some` (the page-loop
     guard asserts it at runtime too).
   - *Round-trip:* `InlineBreakToken::from_cursor ∘ resume` = identity.
   - *Eq/fingerprint laws:* `a == b ⇒ fp(a) == fp(b)`; convergence never
     decided on fingerprint alone.
   - *Monolith:* box > every fragmentainer ⇒ exactly one overflowing
     fragment + one warning, loop terminates.
3. **Bound hindsight:** `relayout_count <= 1` per flow, debug-asserted
   (K33). No other backtracking exists anywhere in the design.
4. **Rollout order:** printpdf/PDF path first (deterministic, corpus-testable,
   no interactive state) → screen paged *preview* second → interactive
   editor (phase 3) last. Each step keeps the previous engine selectable
   until its differential gate has been green across the corpus for a full
   phase.
5. **Standing gates per phase:** the usual battery (layout lib, dll lib,
   `cargo test -p azul-doc` incl. the print corpus, reftests 45/52 with
   zero baseline regressions) — the reftest suite never sees tokens
   (continuous media) and thus doubles as the screen-path no-perturbation
   proof.

---

## 7. Phasing

| Phase | Contents | Exit criterion |
|---|---|---|
| **K30a** | `break_token.rs` types + fingerprints + `BreakCursor` bridge + property tests (no engine changes) | property suite green in isolation |
| **K30b** | `FragmentainerSpace` in `LayoutConstraints` (`None` = today, bit-for-bit); `layout_bfc` break/resume for plain block stacks + IFC children; monolith overflow + warning | resume-law + conservation tests green on block/text fixtures |
| **K30c** | page-loop driver + `PaginationEngine` switch + printpdf consumption; differential harness vs slicer | differential gate green on the provably-correct corpus subset |
| **K31** | margin truncation, Class A/B, forced-break propagation; table rows atomic + thead-at-resume | css-break margin fixtures; two-straddling-tables case beats E18's approximation |
| **K32** | per-(BFC, fragmentainer) `FloatingContext`, float-continuation tokens, page-anchored exclusions | H22 fixture: image drag across a boundary → correct wrap on BOTH pages |
| **K33** | early-break recording + appeals + one bounded relayout; `MonolithWarning` moves layout-time | widows/orphans/avoid fixtures produce NG-grade placements; relayout bound asserted |
| **K34** | token cache in `PaginationSession`; convergence loop from `pagination_dirty_from`; idle-time tail | typing repaginates ≤ 2 pages (counter-asserted e2e); D15-style vview e2e on tokens |
| Phase 3 (post-K34) | screen adoption; slicer + Y-pass + spine-cut retirement; app break-node layer shrinks to forced breaks | slicer deleted; E17/E18 code paths removed |

Nothing in K30a/b touches any shipping path; K30c is the first observable
change and it is opt-in behind the engine enum on the PDF entry only.

---

## 8. Open questions (tracked, not blocking K30a)

1. **`ShapedItem` equality cost.** `partial_remainder` deep-compare on every
   convergence check vs storing `(next_item_index, remainder_len,
   first/last cluster ids, bits-hash)`. Start with derive(PartialEq) +
   fingerprint fast-path; measure on the 100-page corpus before optimizing.
2. **OOF elements.** NG parents fragmented absolutes into the
   *fragmentainer*. v1 keeps today's containing-block behavior (documented
   divergence: an abs-pos box near a boundary renders per its CB, not per
   page). Revisit with K32 (anchored objects share machinery).
3. **Vertical writing modes.** `FragmentainerSpace` speaks *block extent*;
   `WritingModeContext` (fc.rs) already maps main↔physical. Fixtures needed
   before claiming support; v1 gates tokens to horizontal-tb (announce +
   fall back to slicer otherwise).
4. **`page_gap`.** In the token world there is no infinite canvas, so the
   slicer's inter-page dead zone becomes pure presentation (viewer-side
   translation), not layout input. Confirm nothing but the slicer consumes
   `SlicerConfig.page_gap` semantics.
5. **Multicol.** Columns are fragmentainers too — the same loop nests
   (page → column). Out of scope until K31 lands; the design must merely
   not preclude it (it doesn't: `FragmentainerSpace` is not page-specific).
6. **Token remap across structural edits.** Not needed: tokens never
   outlive their layout generation (§4.2); the convergence loop re-enters
   at a pre-edit page whose incoming token is unaffected by construction.
   Assert generation ids in debug builds anyway.

---

## 9. Provenance & license hygiene (per the no-copying-licensed-code rule)

Everything in this document derives from **specs and public architecture
prose — no engine implementation source was consulted**:

- **W3C specs**: css-break-3/4 (fragmentation model, parallel flows, margin
  truncation, break classes), css-page-3, css-multicol-1.
- **Blink LayoutNG**: the public `developer.chrome.com` RenderingNG
  fragmentation article (Stenshorne) and the `layout_ng.md` architecture
  README — both prose documents published to explain the design (the README
  lives in the Chromium tree but is documentation, not implementation).
  The token/constraint-space/fragment shape, appeal scores, the
  one-relayout rule, and monolith-overflow all come from these.
- **Gecko**: Mozilla's public LayoutOverview docs + Continuation_Model wiki
  (the cautionary-tale section).
- **WebKit**: architectural classification only (flow threads ≈ our
  slicer), from public changeset descriptions and the blink-dev mailing
  list (Regions removal) — no WebKit source, no implementation detail used.
- The **finished-siblings invariant** is attributed upstream to Chromium
  commit *messages* (public prose). Independently: if siblings before the
  first token child were unfinished, resume would re-lay them (duplication
  / non-termination) — we enforce it with our OWN debug assertion and
  property tests (§6.2), not by trusting the attribution.
- **K34 token convergence** is derived from the token algebra alone
  (tokens are `Eq` values ⇒ an unchanged incoming token fixes every later
  page) — deliberately NOT from the reflow-convergence description that
  the research doc attributes to ONLYOFFICE source (AGPL); that attribution
  is quarantined and unused here.

The implementation phases must keep this bar: css-break-3 text + this doc
+ black-box A/B against Chrome print output as the oracle (same
methodology as the hinting work: `no-copying-licensed-code.md`).

---

## 10. Appendix — seam inventory (audited 2026-08-04)

- `layout/src/text3/cache.rs:11460` — `BreakCursor<'a>` (items,
  next_item_index, partial_remainder, word_break/hyphens/strictness).
- `layout/src/text3/cache.rs:6382` — `layout_flow(…, flow_chain:
  &[LayoutFragment]) -> FlowLayout`; `:5885` `LayoutFragment { id,
  constraints }`; `FlowLayout { fragment_layouts, remaining_items }`.
- `layout/src/solver3/fc.rs:1060` — `layout_bfc(ctx, tree, text_cache,
  node_index, constraints, float_cache) -> BfcLayoutResult`; `:131`
  `LayoutConstraints`; `:88` `BfcLayoutResult`; `BfcState { pen, floats,
  margins }`.
- `core/src/paged.rs:24` — `FragmentationContext { Continuous, Paged }`
  (metadata-only today; its doc comment states slicing is post-hoc).
- `layout/src/solver3/mod.rs:207` — `LayoutContext.fragmentation_context:
  Option<&mut FragmentationContext>` (the existing thread-through).
- `layout/src/solver3/page_breaks.rs` — `compute_page_breaks*`,
  `BreakPolicy`, `snap_break_up`, `MonolithWarning` (E21),
  `PageConstraints`, sequence-aware forward pass.
- `layout/src/solver3/display_list.rs` — slicer
  (`paginate_display_list_with_breaks`), E17 marker re-derivation
  (`is_push_marker`/`matching_pop`, lazy chains), E18 x-aware thead bands,
  `SlicerConfig { page_sequence, … }`.
- `layout/src/solver3/pagination.rs:643` — `PageSetup::content_height()`;
  `TableHeaderTracker::straddling_tables_for_page`.
- `layout/src/solver3/paged_layout.rs:1-11` — the fossil comment
  ("page_index assigned DURING layout"); `:985`
  `compute_sectioned_pagination` (spine-cut stepping stone);
  `PaginationSession`/`BreaksDelta` (bit-exact prefix, B7);
  `pagination_to_dom_breaks` (A1).
- `../pdf2html/research-fragmentation-web.md` — spec model (§1), NG
  reference design + Gecko cautionary tale (§2), Word-editor prior art
  (§3), the float-across-boundary hard case (§4), synthesis (§5).
- `../pdf2html/AZUL-STILL-TODO.md` §K — K30–K36 as ordered.
