# The content-overlay refactor: one journal, one resolver, seven backends that can't get it wrong

> 2026-07-31. Companion to `/home/fs/Development/pdf2html/PLAN-changesets-pagebreaks.md` (workstreams
> A/B land ON TOP of this). Evidence for every "this breaks today" claim: the 2026-07-31 seam audit
> and resource-flow sweep (memory: `azul-seam-audit-2026-07-31`, `azul-resource-architecture`).

## 1. Why this refactor, stated as the bug classes it deletes

The DOM is immutable by design; mutable state lives in overlays. The design is right. What's wrong
is that there are **many overlays, many write paths, and many read paths**, and every combination
that misses one is a shipped bug. This session alone:

| Shipped bug | The miss |
|---|---|
| Camera/screenshare/video updates invisible on CPU | writer branched on GL context inside a WIDGET (`capture_common.rs:97`); the CPU store was a different mechanism |
| Canvas can't repaint on CPU backend | `RenderImageCallback` results parked in a SIDE MAP (`cpu_image_callback_results`) the damage diff never sees; invoker called on only 4 of 8 backends |
| Maps/video blanked by caret blink | placeholder→VirtualView swap existed in ONE of the two DL-build paths |
| Frozen video after RefreshDom | widget merge returned the wrong allocation; worker wrote into an orphan |
| Inline `<img>` never updates | `InlineContent::Image` SNAPSHOTS the ImageRef at IFC build (`fc.rs:7781`); DL re-reads the snapshot |
| id-registered images take effect "sometime later" | `AddImageToCache` returns `DoNothing`; two mirrored caches; different relayout paths read different ones |
| Exports see pre-edit text | `dirty_text_nodes` never commits/GCs (pdf2html E7) |
| Enter can't split a paragraph | overlay vocabulary has no structural entries (pdf2html E1) |
| `ChangeNodeImage` mutates `StyledDom` in place | `set_node_type` (`event.rs:2114`) — violates the immutability rule the rest of the system is built on |

None of these are per-platform bugs, but all of them SHIP per-platform because each of the 7 event
loops assembles the pipeline by hand. The refactor's goal is structural: **backends physically
cannot participate in content state.** One write chokepoint, one read resolver, one retention
clock — all in shared code (`LayoutWindow` + `shell2/common`). After it, this bug class requires
editing exactly one file to reintroduce.

## 2. What already exists and is KEPT (validated)

- `dirty_text_nodes: BTreeMap<(DomId, NodeId), DirtyTextNode>` — the text overlay, with the single
  documented mutation point (`update_text_cache_after_edit`). Correct shape; becomes one arm of the
  unified overlay.
- `managers/changeset.rs` (964 lines): `TextChangeset`/`TextOperation` record-then-apply, with
  preventDefault inspection. Correct; `DocumentOperation` (pdf2html A1) slots beside it.
- `UndoRedoManager`: per-node stacks of `UndoableOperation { changeset, pre_state }`.
- Generation identity: `NodeIdRemap` + `remap_dom_keys` + `calculate_contenteditable_key`.
- `dom_to_layout: BTreeMap<NodeId, Vec<usize>>` — ALREADY a multi-map: one DOM node may own several
  layout subtrees. This is the pre-existing seat for overlay node splits (§6).
- `needs_paint_only` dirty tier in `solver3/cache.rs:503` — image content swaps repaint without
  relayout. The image overlay rides this.
- `retained_author_css` on the property cache — re-styling reconstructions is possible.
- ImageRef identity (`get_hash`, never reused) — damage by identity (landed f25b72f39).

## 3. Ownership: journal vs undo/redo (the user's question, answered from the code)

Two histories with different lifetimes and different consumers — they must NOT be one structure:

- **`ContentJournal` (new, owned by `LayoutWindow`)** — the mechanical, frame-scoped record:
  ring buffer of `JournalEntry { frame_seq: u64, change: AppliedChange }`. Its ONLY jobs:
  (a) let the compositor reach content as of frame `N−k` (old ImageRef for backbuffer/partial-present
  composition, ≤ N frames), (b) drive convergence GC at generation swaps, (c) drive damage.
  Retention is bounded by the PRESENT loop (`frame_seq` incremented in `shell2/common`, one place):
  entries older than `swapchain_depth` frames (wl_shm double-buffer ⇒ 2; make it
  `max(per-backend buffer count) = 3`) are retired unconditionally. It never grows with document
  size or session length.
- **`UndoRedoManager` (existing) — user-intent history**: grouping, per-node stacks, redo. It does
  NOT own retention and does NOT own pixels. Both are fed by the SAME chokepoint (§4): `apply()`
  writes the journal always, and forwards an `UndoableOperation` when the change is user-undoable
  (text/structural edits yes; per-frame camera frames obviously no — an `Undoability` flag on the
  change, decided by the change constructor, not the caller).

Rule of thumb the code should enforce: *journal = what the RENDERER may still need; undo = what the
USER may still want.* A `Structural` undo entry stores the inverse `DocumentOperation`
(pdf2html A6) — undoing re-RECORDS the inverse through the same loop; it never mutates.

## 4. The write chokepoint

```rust
// layout/src/overlay.rs (new)
pub enum ContentChange {
    Image      { node: (DomId, NodeId), image: ImageRef },              // camera/screenshare/video/canvas
    ImageById  { id: AzString, image: OptionImageRef },                 // css-id cache add/remove
    Text       { node: (DomId, NodeId), edit: TextChangeset },          // existing text path
    Structural { changeset: DocumentChangeset },                        // pdf2html A1 (+ overlay split, §6)
    NodeCss    { node: (DomId, NodeId), props: Vec<CssProperty> },      // restyle overlay (later stage)
}

impl LayoutWindow {
    /// THE single entry: validates, writes the overlay arm, journals,
    /// forwards to undo when undoable, computes the dirty tier
    /// (paint-only vs relayout vs DL-rebuild), and returns what the frame
    /// loop must do. NO OTHER PATH may write content state.
    pub fn apply_content_change(&mut self, change: ContentChange) -> ContentChangeResult;
}
```

- `CallbackChange::{ChangeNodeImage, AddImageToCache, RemoveImageFromCache, ChangeNodeText, …}`
  handlers in `shell2/common/event.rs` become one-line delegations. The per-backend shells never
  see any of this — they receive only the returned dirty tier (the same
  `ProcessEventResult` they already consume).
- `ChangeNodeImage` STOPS calling `set_node_type` on `StyledDom` (deletes the in-place DOM
  mutation) and STOPS calling `regenerate_display_list_for_dom` (an image swap is
  `needs_paint_only` — the resolver (§5) + id-based damage make repaint follow automatically; DL
  items are stable).
- `AddImageToCache` stops being `DoNothing`: the chokepoint computes which nodes resolve that css
  id (reverse index built at styling time) and returns the correct tier.
- The widget rule becomes trivial and enforceable: **widgets only ever call
  `change_node_image` / `record_*` — never a GPU/CPU branch** (`capture_common.rs:97`'s GL branch
  moves INTO the chokepoint's GPU arm: on GPU backends the image arm may additionally update the
  texture key; the widget cannot know or care).

## 5. The read resolver

```rust
/// The ONE lookup order, defined once, borrowed by every consumer:
/// overlay (updatable) → DOM (immutable). The user-stated rule.
pub struct ResolvedContent<'a> { overlay: &'a ContentOverlay, styled_dom: &'a StyledDom, /*…*/ }

impl ResolvedContent<'_> {
    pub fn image_for_node(&self, node: NodeId) -> Option<&ImageRef>;       // overlay.images → NodeData
    pub fn image_for_css_id(&self, id: &AzStr) -> Option<&ImageRef>;       // single map (shell copy DELETED)
    pub fn text_for_node(&self, node: NodeId) -> TextSource<'_>;           // overlay parts → DOM text
    pub fn parts_for_node(&self, node: NodeId) -> &[OverlayPart];          // §6; empty = 1:1
    pub fn image_as_of(&self, node: NodeId, frame_seq: u64) -> Option<&ImageRef>; // journal, ≤N frames
}
```

Consumers that must be rewired to it (this is the whole "7 backends at once" guarantee — none of
these live in backend code):
1. **DL build, block path** (`display_list.rs:3804`) — image via resolver.
2. **DL build, inline path** (`display_list.rs:4509`) — `InlineContent::Image` stores the NodeId,
   resolver at build time. Deletes the stale-snapshot class.
3. **IFC build** (`fc.rs:7781`) — snapshot the NodeId, not the ImageRef.
4. **cpurender raster** — already reads the DL only; with 1–3 the side map
   `cpu_image_callback_results` becomes writable INTO the DL by the callback invoker
   (`invoke_cpu_image_callbacks` rewrites the item's ImageRef through the chokepoint) and the side
   map + `with_image_callback_results` are DELETED. Invoker moves to shared frame code so
   headless/e2e/android/ios get it for free (today: 4 of 8 backends call it).
5. **Hit-test/caret** (`CpuHitTester`, text3 cursor) — text/parts via resolver.
6. **A11y snapshot + accesskit builder** — text via resolver (deletes the
   `dirty_text_overrides` workaround).
7. **Exports**: `get_styled_dom_clone`/`reconstruct_dom_subtree`/`Pdf::*` gain the overlay-merged
   variant (`styled_dom_with_edits`, pdf2html A5.3) implemented ON the resolver.
8. **WR translator** (GPU) — image keys from resolver output; epoch bumps from journal entries.

## 6. "Fake structural edits": the split overlay (the user's `NodeId → {Gen2a, Gen2b}`)

Goal: Enter feels instant WITHOUT waiting for the app's regenerate, and without DOM mutation.

```rust
pub struct OverlayPartId(pub u64);              // "NodeIdGen2": minted from a monotonically
                                                // increasing per-window counter; NEVER a real NodeId
pub struct OverlayPart {
    pub id: OverlayPartId,
    pub content: Vec<InlineContent>,            // this part's inline content (the split halves)
    pub source_range: (usize, usize),           // byte range of the ORIGINAL node's text it covers
}
pub struct NodeSplit {
    pub parts: Vec<OverlayPart>,                // ≥2; order = document order
    pub pending_changeset_id: u64,              // ties to DocumentChangeset (pdf2html A5.2 handshake)
}
// overlay.splits: BTreeMap<(DomId, NodeId), NodeSplit>
```

- **Record**: `DefaultAction::SplitBlockAtCursor` (pdf2html A2) → the chokepoint records the
  `DocumentChangeset` for the app AND (new, this plan) materializes the split in the overlay:
  the node's current content (overlay-first!) is partitioned at the caret into two `OverlayPart`s.
- **Layout**: the layout-tree builder consults `parts_for_node`; k parts → k block-level layout
  subtrees registered under the SAME DOM NodeId via the existing `dom_to_layout` multi-map.
  Each part's IFC shapes independently. (`dom_to_layout` consumers already iterate/`first()` —
  audit each for parts-awareness; the multi-map means the TYPE system already forced them to
  handle >1, most just take `.first()` — those are exactly the caret/hit sites §5.5 rewires.)
- **Caret/hit-test/selection**: `TextCursor` gains the part dimension
  (`cursor.part: Option<OverlayPartId>`); hit entries for split nodes carry
  `(NodeId, OverlayPartId)`; a11y exposes parts as separate paragraphs.
- **Commit** (the app applied the changeset and re-rendered, pdf2html A4/A5): reconcile resolves
  the `EditResumePoint`, sees the new generation has REAL nodes for the parts, and DROPS the split
  entry (convergence GC — same rule as text: overlay entry dies when the DOM catches up). An app
  that re-renders WITHOUT applying (rejected edit) → split entry dropped, content reverts,
  debug-warn (pdf2html A5.2).
- **Undo**: the inverse (`MergeBlocks`) recorded through the chokepoint (§3).
- Merges are the same machinery with k=1 across two source nodes
  (`overlay.merges: BTreeMap<survivor, MergedFrom>`), staged after splits work.

## 7. Image retention semantics (the "≤ N frames" requirement)

`ContentChange::Image` journals `(frame_seq, node, OLD ImageRef)`. Consumers:
- **Partial present / backbuffer composition**: a backend re-presenting a not-fully-redrawn buffer
  composed `k` frames ago may still SAMPLE the old image via `image_as_of(node, seq)` — today this
  is implicit "the old DL is still around", which is exactly the ABA fragility the id-equality fix
  noted. N = max swapchain depth across backends (3). Retire = drop the ImageRef handle (refcount
  frees pixels when the last frame stops referencing them).
- **Damage**: old vs new hash comparison comes from the journal entry, not from diffing retained
  display lists (removes one reason to keep `previous_display_list` clones alive).

## 8. Staging (interleaved with pdf2html's sequencing; every stage ships alone)

| Stage | Contents | Deletes | Risk |
|---|---|---|---|
| **O1** | `ContentOverlay`+`ContentJournal`+chokepoint; IMAGE arm only (node images + css-id map); resolver wired into DL build (block+inline+IFC-by-NodeId); invoker moved to shared code | `set_node_type` DOM mutation; `regenerate_display_list_for_dom` on image swaps; `cpu_image_callback_results`+`with_image_callback_results`; SHELL `common.image_cache` copy + mirroring; `capture_common` GL branch | M — touches DL build; golden: e2e corpus + new `op-image-swap-repaints` scenario |
| **O2** | TEXT arm: `dirty_text_nodes` folds into overlay (same entries, new home); convergence GC at reconcile (=A5.1); `styled_dom_with_edits` on the resolver (=A5.3); exports rewired | the "remapped forward forever" leak; a11y `dirty_text_overrides` | S–M |
| **O3** | STRUCTURAL arm: `DocumentOperation` vocabulary (=A1) + split overlay (§6) + caret/hit/a11y part-awareness + commit handshake (=A5.2) + undo inverse entries (=A6) | — (pure addition) | L — the layout-tree parts integration is the deep work |
| **O4** | `NodeCss` arm (restyle overlay) + journal-driven damage everywhere; delete `previous_display_list` retention where the journal supersedes it | DL-clone diffing for content changes | M |

Ordering vs pdf2html: **B1 → O1 → A5≡O2 → B2 → A1+O3 → A2–A4 → B3 → A6 → B4/O4.**
O1 first because it deletes the largest live-bug surface (all image classes) with no new concepts;
O2 subsumes A5; O3 is what makes A2's Enter-split feel instant instead of waiting a full app
round-trip.

## 9. Enforcement (what makes the classes *architecturally* impossible)

1. **Privacy**: overlay fields are private to `layout/src/overlay.rs`; `apply_content_change` is
   the only `pub` writer. Backend crates (`shell2/*`) get no API that takes an ImageRef or text —
   grep-clean CI check: `rg 'set_node_type|image_cache|dirty_text_nodes' dll/src/desktop/shell2 --glob '!common/*'`
   must return nothing (add as a CI lint step).
2. **One resolver**: `ResolvedContent` constructed in exactly two functions (DL build entry, export
   entry). Any consumer needing content takes `&ResolvedContent`, not `&StyledDom`.
3. **Frame clock**: `frame_seq` bumps in `shell2/common` present orchestration only. Backends
   can't hold content back because they never see it — they blit what the shared layer hands them.
4. **Invariant tests** (e2e ops exist for all of these): `op-image-swap-repaints` (change image →
   assert damage non-empty + screenshot changed, on headless — which today has NO working callback
   images at all), `noninterference-overlay-*` (image swap moves nothing else),
   `assert_only_managers_changed` gains `overlay`/`journal` rows, split scenario per pdf2html A7.
5. **Announce-on-degrade** (parallel effort, in flight): any capability arm that is compiled out or
   fails to load says so once, loudly — silent Nones are treated as bugs, same as this table.

## 10. Open questions (decide during O1, none block the start)

- Does `ImageById` need per-DOM scoping (VirtualView child DOMs resolving the host's css ids)?
  Today's cache is global; keep global until a demo needs otherwise.
- `NodeCss` arm vs the existing `restyle_user_property` path — likely the same code moved, not new
  code; verify the property-cache write is already single-sited.
- GPU epoch integration: whether journal entries carry the WR `Epoch` bump or the translator derives
  it — decide when rewiring §5.8.
- Whether `OverlayPartId` should fold into `az_children`-style iteration for a11y ordering or stay
  a parallel dimension — prototype in O3.

## Amendment (2026-07-31, review): structural edits are TREE ops, not text ops

§6's original sketch (`OverlayPart { content: Vec<InlineContent>, source_range: (byte, byte) }`)
and the A1 vocabulary (`SplitBlock { at: TextCursor }`, `xml_fragment` payloads) were
text-specific shortcuts and were REPLACED during implementation:

- The split/join coordinate is **`NodePosition { child_index, text_byte: Option }`** —
  element children move wholesale, only a text child is ever cut. A `<ul>` splits
  between `<li>`s with the same op that splits a `<p>` mid-word.
- Content payloads are **native `Dom` subtrees with fragment semantics** (root
  ignored, children inserted), never markup strings. The op set is
  `SplitNode / MergeNodes / InsertChildren / RemoveChildren / ReplaceChildren`
  (+ the explicitly text-specific `Wrap/UnwrapRange`), closing its own inverse
  algebra.
- The overlay preview stores the **recorded delta itself** (`StructuralPreview`),
  no copied content; `ResolvedContent::children_for_node` yields the ADJUSTED
  child list (`Existing | ExistingTextSlice | Pending`) — the immutable-DOM
  equivalent of `.insertChild()` becoming visible before the app's re-render.
- `EditResumePoint` = `{ anchor_key (any stable node), node_path, position }`.
- The apply helper (`document_edit.rs`) operates on `Dom`, not XML.
