# Selection architecture review — branch `fix/input-bugs-2026-09-19` (PR #476)

Repo root: `/Users/fschutt/Development/azul`. All paths below are relative to it.
Method: read-only code review + `git show` of the bug-class commits. **Nothing was
compiled or run** (per instructions). Every live-bug claim in §5 is stated as a
falsifiable input → output with the test that would confirm it; confidence is
marked. Line numbers are from the working tree at `97f211f96`.

---

## 0. TL;DR — three root causes behind the whole bug class

1. **"Which node does this selection live on?" is a bare `NodeId`/`DomNodeId` with
   ~5 meanings and ~30 hand-written resolvers.** `MultiCursorState.node_id` is the
   IFC root when the click path opens a session, the *text leaf* when focus opens
   it, an arbitrary element for the resume point / app API, and the host for a11y.
   Every consumer re-resolves it with its own rule (walk up to "any layout node",
   walk up to "own inline layout", walk down BFS caret-first, exact key match, …).
   Anonymous IFC roots (`dom_node_id: None`) cannot be named at all.
2. **A `TextCursor`'s `source_run`/`start_byte_in_run` is minted in one index space
   and consumed in another.** Cursors come from the shaped layout (runs of the
   content `solver3::fc` built: `::marker`, `<br>`→`LineBreak`, atomics, and
   *white-space-collapsed* text). ~20 consumers index
   `get_text_before_textinput` (a DOM-child walk with raw text, no marker, no
   atomics) with them. e1b0099e3 fixed exactly one of these (cross-block copy).
   On top of that there are two caret conventions (Leading@len vs Trailing@last)
   compared with structural `==`.
3. **Two parallel selection stores with independent lifetimes:**
   `TextEditManager.multi_cursor` (single node) and `TextEditManager.cross_block`
   (render-ready multi-block cache). `cross_block` "wins" for paint/copy/delete, but
   only 5 code paths ever clear it — a click, Tab, arrow keys, typing and focus
   changes do not. Each consumer decides separately which store to read.

Everything in the recent history maps onto one of these (table in §2.4).

---

## 1. Inventory — every identifier that says "where a selection/caret lives"

| # | Identifier (type) | Meaning(s) actually carried | Producers | Consumers |
|---|---|---|---|---|
| 1 | `DomId` (`core::dom`) | which DOM (root / VirtualView child / popup) | `layout_results` keys | everything; `CopyToClipboard` falls back to `DomId{inner:0}` (`event.rs:7537-7540`) |
| 2 | `NodeId` (arena index, `usize` newtype) | DOM node — used interchangeably for **IFC root element, text leaf, inline element, host, block, list-item** | everywhere | everywhere; no type distinguishes the roles |
| 3 | `NodeHierarchyItemId` (1-based encoded `Option<NodeId>`) | inside `DomNodeId`; decoded with `into_crate_internal()` 62× / encoded 69× in `window.rs` alone | — | — |
| 4 | `DomNodeId {dom, node}` | focus target, **session node** (`MultiCursorState.node_id`), `SeatCaret.node`, tween node/scope, changeset target, host | focus mgr, `initialize_editing`, app API | see §2.1 |
| 5 | `LayoutNodeId` **and** raw `usize` layout index | layout-tree box. APIs mix both: `warm(LayoutNodeId)`, `get_inline_layout_for_node(usize)`, `get_ifc_root_layout_index(usize)->usize`, `IfcMembership.ifc_root_layout_index: usize`, `pos_get(..usize)`; 38 `LayoutNodeId::new(` in `window.rs` | layout tree | geometry, selection |
| 6 | `dom_to_layout: NodeId → Vec<LayoutNodeId>` | 1→many: principal box, the `::marker` pseudo box (same `dom_node_id`!, `fc.rs:9119-9124`), split-preview parts (`contenteditable_e2e.rs:1519`) | layout tree build | callers take `.first()` (31 sites) or `nodes.iter().position(dom_node_id==)` (5 sites) — two different "first" rules |
| 7 | `LayoutNode.dom_node_id: Option<NodeId>` | `None` for **anonymous** boxes, incl. anonymous IFC roots (`layout_tree.rs:2668-2672, 2709`) | `create_anonymous_node` | every selection path filters `None` out (§5 #10) |
| 8 | `IfcMembership {ifc_id, ifc_root_layout_index: usize, run_index: u32}` (`layout_tree.rs:89-98`) | set only on text nodes the IFC root walks **directly** (not under `<span>`); `run_index` counts *text DOM nodes* (`fc.rs:8820-8823, 9329-9333`), although documented as "Maps to `ContentIndex::run_index`" — wrong whenever a marker/`<br>`/atomic or a split text precedes | `fc::collect_and_measure_inline_content` | only `ifc_root_layout_index` is read |
| 9 | `ContentIndex {run_index, item_index}` | text3: run = index into IFC content vec, item = byte in run. **fc child_map for anon wrappers**: `run_index = ifc_root_index` (a LAYOUT index) and `item_index = DOM child index` (`fc.rs:8768-8771, 9261-9264`) — a third meaning for the same struct | fc, text3 | text3, paged_layout, break tokens |
| 10 | `GraphemeClusterId {source_run, start_byte_in_run}` | run index **in the fc-built content** + byte in that run's (possibly collapsed) text | shaping | 183 `source_run` / 217 `start_byte_in_run` src uses |
| 11 | `TextCursor {cluster_id, affinity}` (FFI, in api.json) | two conventions: edit path Leading@len (`insert_text` 580-586, `end_of_content_cursor` 16902), layout Trailing@last (`get_last_cluster_cursor` 6127, `end_cursor` 5582, hittest right half 5751-5755); derived `Eq/Ord` is **structural**, not positional | text3, edit | everywhere |
| 12 | `SelectionRange {start, end}` (FFI) | start = **anchor**, end = **focus**, not normalized (`text_edit.rs:364-371`) | drag, keyboard | Wayland reads them as (cursor, anchor) — swapped (§5 #9) |
| 13 | `MultiCursorState {selections, primary_id, node_id: DomNodeId, contenteditable_key}` (`core/selection.rs:396-407`, not in api.json) | single-node session; `node_id` meaning depends on producer (§2.1) | click / focus / a11y / resume / cross-block replace / handle drag / app API | 78 `mc.node_id` src uses |
| 14 | `TextSelection {dom_id, anchor, focus, affected_nodes: BTreeMap<NodeId,Vec<SelectionRange>>, remote_ranges, is_forward}` (`core/selection.rs:1068-1104`, not in api.json) | documented key = "IFC root NodeId"; `build_primary_text_selections_map` fills it with `mc.node_id` (can be a text leaf) (`text_edit.rs:1567-1568`) | `set_cross_block_selection`, `build_text_selections_map` | display list, copy, delete, handles |
| 15 | `SelectionAnchor/SelectionFocus {ifc_root_node_id: NodeId, cursor, char_bounds, mouse_position}` | `char_bounds`/`mouse_position` are **always zero** (dead fields: `window.rs:3794-3800`, `text_edit.rs:1581-1587`) | same | same |
| 16 | `SessionSelectionRanges.node_id`, `RemoteSelectionRanges.node_id` (`text_edit.rs:327-360`) | doc: "IFC root every range is expressed against"; value: `mc.node_id` | text_edit | paint map |
| 17 | `CursorLocation {dom, node: NodeId, cursor, owner,…}` (`text_edit.rs:397-410`) | session node or seat node | `build_cursor_locations` | `paint_cursor` (ownership-resolved) |
| 18 | `SeatCaret {node: DomNodeId, cursor, anchor}` | seat's node (text leaf per display_list comment 4164-4166) | seat ops | seat copy/delete/paint |
| 19 | `TextTweenState.node / focus_scope: Option<DomNodeId>` | session node / focusable ancestor | `initialize_editing`, `enter_focus_scope` | tween |
| 20 | `contenteditable_key: u64` | stable host identity (`calculate_contenteditable_key` of the **host**) | `contenteditable_session_key` (3158) | resume point, generation shift |
| 21 | `PendingContentEditableFocus {dom_id, container_node_id, text_node_id}` | host + **last text leaf** (or last empty IFC, or host) | `handle_focus_change_for_cursor_blink` (9690-9693) | `finalize_pending_focus_changes` → becomes the session node (10146-10151) |
| 22 | `DocumentPosition {node, text_byte}` / `DocumentSelectionSpan` (FFI) | byte in the **flattened** node text (`get_node_text_content`) | `resolve_cursor_to_text_byte` | app sync API |
| 23 | `NodePosition {child_index, text_byte}` / `EditResumePoint` | DOM child index + byte in *that text child* | structural edits | `restore_caret_from_resume_point` mints `(run 0, that byte)` (4454-4499) |
| 24 | `RunTextChange {run,…}`, `RunRemap`, `RunTextDiff` | runs of the **edit-model** vector | `run_text_diff(get_text_before_textinput…)` | applied to layout-space cursors (`shift_carets_across_generation` 4387-4413) |
| 25 | `text_selection_drag_anchor: Option<LogicalPosition>` | window-space press point, "a selection gesture is in flight" | dll press edge (`event.rs:11337-11355`) | f807d2523 focus guard, drag dispatch |
| 26 | `SelectionHandleDrag {end, anchor: TextCursor, cross_block}` | other end of the handle drag | `begin_selection_handle_drag` | handle drag |
| 27 | Hit test: `regular_hit_test_nodes: NodeId→HitTestItem` (tagged **elements** only — text leaves never), `TAG_TYPE_CURSOR` run areas keyed by text node (`display_list.rs:8089-8131`), `point_relative_to_item: ContentBoxLocal` | hit node ≠ IFC root whenever an inline box is hit (5d906bd2f) | hit tester | click path |
| 28 | Byte/offset spaces | `start_byte_in_run` (cluster start, collapsed text); `cursor_byte_offset_in_run` (affinity-resolved); grapheme-stop offsets (`grapheme_caret_offset` 6188); flat node bytes (IME/a11y/app); visual-order cluster byte sums (`byte_offset_to_cursor` 19078); `preedit_cursor_begin/end: i32` with −1 sentinel; `NodePosition.text_byte` | — | see §2.2 |

Positive precedent already in the tree: the **space vocabulary**
(`WindowPoint → static → BorderBoxLocal → ContentBoxLocal → ScrolledContentPoint`,
`ifc_local_point_rebased` 19355) killed the four-divergent-copies problem for
*coordinates* (see the doc comment at 19290-19307). Node identity and run/byte
spaces need the same treatment.

---

## 2. Conversions and duplicated paths

### 2.1 "Node → the IFC that owns its text" — ~30 resolvers, ≥8 distinct rules

| Resolver | Rule | Anonymous IFC | text under `<span>` | text leaf | Notes |
|---|---|---|---|---|---|
| `LayoutTree::get_inline_layout_for_node` (layout_tree.rs:1836) | own ∨ membership; **sparse view** | – | ✗ | ✓ | sentinel under dense default |
| `LayoutTree::get_dense_for_node` (1773) | own ∨ membership | – | ✗ | ✓ | |
| `LayoutTree::get_cached_inline_layout_for_node` (1823) | own ∨ membership | – | ✗ | ✓ | added by 1cb23d016 |
| `LayoutTree::materialized_inline_layout_for_node` (1796) | **own only** | – | ✗ | ✗ | used by paint |
| `LayoutTree::get_ifc_root_layout_index` (1862) | own ∨ membership ∨ **layout-parent walk** | ✓ | ✓ | ✓ | the only "complete" rule |
| `LayoutWindow::get_inline_layout_for_node` (window.rs:10218) | `dom_to_layout.first()` → sparse accessor | ✗ | ✗ | ✓ | 39 call sites |
| `LayoutWindow::materialized_inline_layout_for_node` (10205) | first → cached (own ∨ membership) | ✗ | ✗ | ✓ | |
| `get_node_inline_layout` (19146) **and** `session_geometry_node` (12590) | candidates with own IFC ∨ layout-parent walk (copy-pasted incl. comment) | ✗ | ✓ | ✓ | duplicate pair |
| `CallbackInfo::get_inline_layout_for_node` (callbacks.rs:3264) | first → **own only, sparse** | ✗ | ✗ | ✗ | sentinel → `inspect_move_cursor_*` dead |
| `resolve_ifc_layout_node` (window.rs:10293) | self if sparse-accessor `is_some()` else BFS descendants caret-owner-first | ✗ | ✗ | returns the *leaf* | keyboard ops (ab830a41d) |
| `ifc_candidate_children` (17421) | BFS (not document order!), walled-off excluded, caret owner moved to front | ✗ | – | – | reshape / seed / resolve |
| `caret_text_target` (3467) | DOM-parent walk to nearest **non-text node with ANY layout node** | ✗ | returns the `<span>`/`<b>` | ✓ | edit commits (320921612) — diverges from IFC ownership |
| `structural_edit_node` (3414) | direct child of host on caret chain, if element | – | – | – | Enter/merge |
| `reshape_text_node` (17514-17558) | own IFC ∨ descend candidates; **never ascends** | ✗ | ✗ (no-op) | ✗ | |
| `finalize_pending_focus_changes` (10010-10029) | DOM-parent walk until sparse ∨ dense accessor is Some | ✗ | ✓ | ✓ | 62aa86077 |
| click path (19608-19632) | hit node: own ∨ membership; requires root `dom_node_id` | ✗ | skips span, falls back geometrically | n/a (leaves never hit) | + `is_text_selectable` filter |
| click fallback (19692-19789) | geometric scan of layout nodes with own IFC ∧ dom id | ✗ | – | – | |
| drag path (20005-20013) | `nodes.position(dom_node_id == session)` ∧ **own** IFC | ✗ | ✗ | ✗ bails | requires click-path session |
| `hittest_text_position_global` (20109) | all layout nodes with own IFC ∧ dom id, nearest box | ✗ | – | – | **no user-select / host filter** |
| `ifc_roots_in_document_order` (4042) | layout index order, own IFC ∧ dom id | ✗ | – | – | no user-select / host / walled-off filter |
| Ctrl+A block walk (event.rs:7761-7818) | DOM DFS from host, element with sparse-accessor `is_some()` | ✗ | – | – | lives in the dll, untestable from layout |
| `ifc_root_owns_dom_node` (display_list.rs:4360) | `dom_to_layout` → `get_ifc_root_layout_index`; boxless → DOM ancestor walk | – | ✓ | ✓ | caret, recolour, remote ranges |
| `paint_selections` local ranges (display_list.rs:4163) | **exact key == IFC root's own dom id** | ✗ (returns at 4120) | ✗ | ✗ | diverges from the recolour 20 lines later |
| `selection_runs_for_node` (18727) | dense ∨ materialized | ✗ | ✗ | ✓ | e1b0099e3 |
| `node_text_end_cursor` (3680) | dense ∨ `get_node_inline_layout` | ✗ | ✓ | ✓ | |
| `find_contenteditable_host` (3182) | nearest self-or-ancestor with the own flag | – | – | – | Ctrl+A root, copy fallback |
| `is_node_contenteditable_inherited` (getters) | inheritance with `contenteditable=false` walls | – | – | – | click, paint, press arming |
| dll `press_on_editable` (event.rs:11263-11335) | contenteditable-inherited ∨ (node is text ∨ has a *direct* text child among first 64) ∧ selectable | – | – | – | arms the drag |
| `find_last_text_child` / `find_last_empty_editable_line` (15841/15883) | DFS last text leaf (does **not** skip walled-off islands) | – | returns leaf inside span | – | focus seed |
| `seed_style_node` (17190) | first text among `ifc_candidate_children` | – | – | – | |

Headless and device even disagree on whether a text leaf "owns" a layout
(62aa86077: "HARNESS LIMIT … headlessly the bare leaf DOES own an inline layout"),
i.e. the membership rule is path-dependent — tests pass while the device fails.

### 2.2 Cursor ↔ offset converters — 7 implementations, 5 different answers

| Converter | Content read | Counts non-text items? | Affinity | Order |
|---|---|---|---|---|
| `resolve_cursor_to_text_byte` (2724) → app API | DOM walk of given node | Space/LB/Tab = 1, Marker = len (`inline_item_flat_len` 2704) | ✓ | logical |
| `byte_offset_of_cursor` (13070) → macOS/iOS/Android IME | DOM walk of **session** node | **only Text runs** (LineBreak skipped) | ✓ | logical |
| `ime_surrounding_text` (17315) → Android | DOM walk of **focused host** | `flatten_inline_content` | **✗ (raw `start_byte_in_run`)** | logical |
| Wayland `send_surrounding_text` (wayland/mod.rs:10570-10631) | DOM walk of session node | Text/Space/LB/Tab; Marker 0 | **✗** ; start/end reported as cursor/anchor (swapped) | logical |
| `byte_offset_to_cursor` (19078) ← IME/a11y set | **layout clusters** of the session IFC | clusters only (marker clusters included) | returns Trailing; **offset 0 → Trailing on first cluster (=offset 1)** | **visual** item order |
| `DenseText::byte_offset_to_cursor` (a11y, 16428) | dense | twin of the above | | |
| `seat_selected_text` (10516) / `extract_clipboard_ranges` (20411) | DOM walk | index by `source_run` | ✓ | logical |

### 2.3 "Text content" providers — two index spaces

* **Layout space (A)**: the `Vec<InlineContent>` `solver3::fc` built for the IFC
  (`fc.rs:8710-9700`): `::marker` pushed first when the IFC root or its parent is a
  list-item (9083-9171), `<br>`→`LineBreak` (8831, 9340), inline-blocks→`Shape`,
  images→`Image`, abspos skipped, text through `split_text_for_whitespace`, which
  **collapses** whitespace for `normal`/`nowrap` (10970-11020). It is cached on the
  IFC root as `LayoutNodeWarm::inline_content_cache.content`
  (`layout_tree.rs:226`) — cursors index exactly this vector.
* **Edit space (B)**: `get_text_before_textinput` (16974): overlay first; a Text
  node returns `split_text_for_whitespace` only when its parent is `pre*`, otherwise
  **one raw run** (17063-17068); containers recurse over an allow-list; `Br`,
  `Image`, markers, pseudo nodes → nothing (17179-17182); inline-block `Div`s recurse
  into their text (A has one `Shape` there).
* After the first edit the IFC is reshaped from the **B** vector
  (`update_text_cache_after_edit` → `reshape_text_node`), so the numbering flips
  from A to B until the next full relayout re-runs fc.

Consumers that index **B with A-minted cursors** (the e1b0099e3 class; 42 src
call sites of `get_text_before_textinput`, these are the cursor-indexed ones):
`resolve_cursor_to_text_byte` 2731, `build_editing_query_state_for_seat` 3018,
`caret_node_position_for_seat` 3585, `replace_cross_block_selection` 3870/3893,
`shift_carets_across_generation` 4400, `seat_selected_text` 10508,
`apply_seat_selection_op` 10566, `ime_document` 12857 (+offset from 13078),
`byte_offset_of_cursor` 13078, `select_next_occurrence` 13339,
`apply_one_text_changeset` 16674, `ime_surrounding_text` 17318,
`delete_selection` 20334, `seat_selected_content_for_clipboard` 20473/20477,
`get_selected_content_for_clipboard` single-node 20604/20615, smart paste
`event.rs:7682`, Wayland `send_surrounding_text`. Only the cross-block copy
(20517 → `selection_runs_for_node`) reads A.

### 2.4 Which divergence produced which recent bug

| Commit | Symptom | Divergence (root cause #) |
|---|---|---|
| ab830a41d (08-24) | Backspace/arrows dead in every TextInput | keyboard op used the focused **host** where an IFC was needed (#1) → `resolve_ifc_layout_node` |
| 320921612 / 4555bcfaf (09-03/04) | deletions keyed to host; smart paste keyed to text leaf; Enter painted at old place | commit target = host vs leaf vs IFC owner (#1) → `caret_text_target` (itself a new, 4th rule) |
| 98b64d369, 62aa86077, ef355e810 (08-19..31) | no caret rect / reveal for focus-opened sessions; Tab caret at "4\|2" | session keyed on text leaf; lookup by leaf; Trailing@0 fallback (#1, #2 conventions) |
| eb086526a (08-19) | click placed no caret; span-nested caret not painted | sparse sentinel in click path; membership missing under `<span>` (#1 accessor zoo) |
| 5d906bd2f (08-25) | caret lands where the inline box starts | hit node ≠ IFC root coordinate space (fixed with typed spaces) |
| 28441690d (08-19) | caret/IME/session resolved in root DOM for nested DOMs | `DomId` ignored by accessors (#1) |
| 838adc974 (09-21) | multi-node selection only between siblings | sibling walk vs document order (#3); left `replace_cross_block_selection`'s same-parent assumption in place (§5 #2) |
| f807d2523 (09-22) | left-drag never selects | (2) session (plain text) and focus (host) have independent lifetimes; the focus guard only knew Ranges (#3); (3) chrome selectable — `user-select` honoured by the click path but not the drag/doc-order paths (§5 #11) |
| 1cb23d016 (09-22) | Ctrl+A selects nothing | sparse accessor vs materialized (#1 accessor zoo); the single-block fallback at `event.rs:7894` still uses the sparse one |
| a0d538a4e (09-22) | fast drag freezes after one char | "which node" = pointer-now vs press node (gesture-level identity) |
| e1b0099e3 / 26fa2152d (09-22) | cross-block copy copies nothing for list items | A vs B run index spaces (#2); commit message itself lists the remaining sites |
| 1f71cf82a / 124f50467 (09-22) | drag can't land on a blank line | hit→cursor duplicated: click has 2 branches with `empty_editing_host_caret`, drag had 2 without (#1); anchor-side and Ctrl+A halves remain (§5 #6) |

---

## 3. Multi-node (document-level) selection

**Representation.** Two stores plus seats, no single source of truth:

* `multi_cursor: Option<MultiCursorState>` — one node, many local ranges +
  peer snapshots; the primary's `Range.start` doubles as the drag anchor.
* `cross_block: Option<TextSelection>` — precomputed per-IFC ranges
  (`set_cross_block_selection` 3707-3809): anchor block from cursor to
  `node_text_end_cursor`, middles `(0,0,Leading)..end`, focus block
  `(0,0,Leading)..cursor`. Stored render-ready; **wins wholesale** over
  `multi_cursor` in `build_primary_text_selections_map` (`text_edit.rs:1529-1533`),
  so while it exists the session's own ranges / multi-cursor are not painted.
* `seat_carets` — other seats, single node each, folded in as remote ranges.

The anchor exists twice (mc primary + `cross_block.anchor`), the focus only in
`cross_block`. When the drag leaves the anchor block, mc keeps its last in-block
`Range(anchor, last in-block focus)` (the cross-block branch returns before
updating mc, 20046-20056) — a stale partial range that typing then acts on.

**Anchoring.** `SelectionAnchor.ifc_root_node_id: NodeId` — cannot name an
anonymous IFC; `char_bounds`/`mouse_position` are dead (always zero).

**Extension.** Only the pointer drag (`process_mouse_drag_for_selection`) and the
Android handle drag extend across blocks. Keyboard Shift+Arrow steps inside one
IFC layout only (`apply_selection_op_for_seat` 10412-10434) and, with a
`cross_block` active, silently mutates the invisible mc. No Shift+click.

**Document order — five definitions in use:**
1. NodeId index — `is_forward` (3722), copy sort (20509), delete sort (3859).
2. Layout-tree index — `ifc_roots_in_document_order` (4042) for middles.
3. DOM DFS — Ctrl+A block discovery (`event.rs:7793-7815`).
4. BFS caret-owner-first — `ifc_candidate_children` (17421) (not an order at all:
   `host > [section > p1, p2]` yields p2 before p1).
5. Screen distance — `hittest_text_position_global` ranking.
(1) and (2) agree today only because both happen to be pre-order; a split-preview
part appended out of order would make `ifc_roots[i_first+1..i_last]` a
reversed slice → panic.

**Painting.** Display list per IFC root: local ranges by exact key (4163), recolour
and remote ranges by ownership (4174, 4310), `user-select` checked on the IFC root
(4155), anonymous roots skipped (4120).

**Copy.** Cross-block: fixed in e1b0099e3 (layout-space runs), but sorted by NodeId
and includes `user-select:none` / hidden blocks that are not painted. Single node:
still B-space (§5 #3).

**Delete / replace.** `replace_cross_block_selection` (3841-4016): **takes** the
selection, computes kept head/tail from B-space content, requires both ends to share
a parent (3930, an invariant 838adc974 removed), emits one `ReplaceChildren` of the
parent's `[first..=last]` child range with a clone of the first block holding ONE
plain text child (all inline formatting flattened, "v1").

**Missing / inconsistent** (details in §5): clearing on click/Tab/arrows/typing/
focus change; type-to-replace; cross-parent delete; anonymous IFCs; `user-select`
and editing-host scoping; empty first block as anchor; keyboard extension across
blocks; recomputation after edits (ranges are precomputed and only NodeId-remapped
on reconcile, `text_edit.rs:1707-1737`, never re-measured after a text edit).

---

## 4. Newtype proposals

Cheap vs expensive: `MultiCursorState`, `TextSelection`, `SelectionAnchor/Focus`,
`SessionSelectionRanges`, `CursorLocation`, `SeatCaret` are **not in api.json**
(checked) → internal, free to retype. `TextCursor`, `GraphemeClusterId`,
`SelectionRange`, `DocumentPosition`, `DocumentSelectionSpan` **are** public → keep
them as the FFI shape and wrap internally, converting at the API boundary.

### N1 `TextBlock` — an IFC, proven to own an inline layout
```rust
/// Stable across relayout; remappable through NodeIdMap on reconcile.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextBlock { pub dom: DomId, key: TextBlockKey }
enum TextBlockKey {
    Element(NodeId),                              // the IFC root element
    Anonymous { parent: NodeId, first_child: NodeId }, // anon InlineWrapper
}
impl TextBlock { fn layout_index(&self, tree: &LayoutTree) -> Option<LayoutNodeId>; }
impl LayoutWindow {
    /// THE rule: own ∨ membership ∨ layout-parent walk ∨ (boxless) DOM-ancestor
    /// walk — i.e. today's `ifc_root_owns_dom_node` + `get_ifc_root_layout_index`.
    fn text_block_of(&self, node: DomNodeId) -> Option<TextBlock>;
}
```
* Signature changes: `MultiCursorState.node_id: DomNodeId` → `block: TextBlock`
  (plus N2 `host`); `TextSelection.affected_nodes: BTreeMap<TextBlock,…>`;
  `SelectionAnchor/Focus.ifc_root_node_id` → `block`; `CursorLocation.node`,
  `SeatCaret.node`, `SessionSelectionRanges.node_id`; `initialize_editing(cursor,
  TextBlock, key)`; `set_cross_block_selection(TextBlock, cursor, TextBlock, cursor)`;
  `hittest_text_position_global -> (TextBlock, TextCursor)`.
* Would have made impossible: 98b64d369 / 62aa86077 / ef355e810 (session on a
  leaf), the span half of eb086526a, `caret_text_target` returning `<b>` (§5 #7),
  the paint exact-key miss (§5 #5), the drag bailing on a non-IFC session
  (20011-20013), anonymous IFCs being unnameable (§5 #10), resume-point sessions on
  a text child (§5 #14).
* Cost: `mc.node_id` 78 src + 7 test; `affected_nodes` 60 + 12; `ifc_root_node_id`
  58 + 2; `initialize_editing(` 24 + 28; `set_cross_block_selection(` 10 + 12;
  `CursorLocation {` 3; seat caret uses 33; `cross_block` 71. ≈ 330 edits, almost
  all mechanical once `text_block_of` exists; remap code (`text_edit.rs:1607-1737`)
  becomes one `TextBlock::remap(&NodeIdMap)`.

### N2 `EditHost` — the contenteditable host (focus target, undo key)
```rust
pub struct EditHost(DomNodeId); // minted only by find_contenteditable_host
```
* Changes: `delete_selection(EditHost, forward)`, `apply_selection_op(_for_seat)`,
  `record_text_edit_undo(EditHost,…)`, `build_editing_query_state(EditHost)`,
  `contenteditable_session_key(EditHost)`, `SystemChange::{ApplySelectionOp,
  CutToClipboard, UndoTextEdit, RedoTextEdit}.target`, `caret_text_target` is
  deleted (the session already carries its `TextBlock`).
* Prevents: ab830a41d and 320921612/4555bcfaf (host passed where an IFC was
  needed); §5 #8 (query state reading the host's flattened text with an IFC
  cursor); §5 #9 (`ime_document`/`ime_surrounding_text` using the focused host's
  text with a session offset); §5 #1 (`delete_selection` could assert that the
  cross-block selection belongs to this host).
* Cost: `caret_text_target(` 4, `find_contenteditable_host(` 7, SystemChange
  producers/consumers ≈ 20, `delete_selection`/`apply_selection_op` callers ≈ 15.

### N3 One content model per block (or phantom-typed run indices)
Best: make the IFC's cached layout content (`inline_content_cache.content`,
overlay-first) the **only** thing a cursor may index:
```rust
pub struct BlockContent { block: TextBlock, runs: Arc<[InlineContent]> } // layout space
impl LayoutWindow { fn block_content(&self, b: TextBlock) -> BlockContent; }
// get_text_before_textinput stays, renamed `flattened_text_of(node)`, returning a String:
// app/a11y reads only, never indexed by a cursor.
```
Minimum: `struct RunIdx<S>(u32, PhantomData<S>)` with `S = Layout | Edit` behind
the internal APIs, so `content.get(cursor.run())` does not type-check against a
DOM-walk vector.
* Prevents the whole e1b0099e3 class (§5 #3, #4 and the `document_selection_spans`
  / Ctrl+D / generation-shift variants).
* Cost: ~20 cursor-indexed call sites of 42; `text3::edit` is already generic over
  `&[InlineContent]`, so the edit functions do not change — only which vector they
  are handed.

### N4 Positional caret equality (`CaretPos`)
```rust
/// Canonical: Leading on the stop that begins here, Trailing only at block end
/// (`cursor_from_grapheme_offset` 6211 already defines this).
pub struct CaretPos(TextCursor);
impl TextTarget { fn canonical(&self, c: TextCursor) -> CaretPos; fn at_start/at_end(&self, CaretPos) -> bool; }
```
* Replace structural `==`/field tests at: drag `anchor == focus` (20084), handle drag
  (13020, 13031), `at_start`/`at_end` (3019-3038), finalize seed (10123-10129),
  `byte_offset_to_cursor(0)` (19080-19097).
* Prevents ef355e810 ("4|2"), §5 #8, #12, the offset-0 half of #9. Cost ≈ 10 sites.

### N5 Offset newtypes + one converter pair
```rust
pub struct FlatByte(u32);   // byte in the node's flattened text (app, IME, a11y)
pub struct RunByte(u32);    // byte within one run of a BlockContent
impl TextTarget {
    fn flat_byte_of(&self, CaretPos) -> FlatByte;
    fn caret_at(&self, FlatByte) -> CaretPos;   // logical order, never visual
}
```
Replaces the 7 converters in §2.2 (`resolve_cursor_to_text_byte`,
`byte_offset_of_cursor`, `ime_surrounding_text`, Wayland `send_surrounding_text`,
`byte_offset_to_cursor`, `DenseText::byte_offset_to_cursor`, seat inline copy).
Prevents §5 #9. Cost ≈ 20 callers (macOS 4, iOS 5, Android 1, Wayland 2, window ~8).

### N6 One document order
```rust
pub struct BlockFilter { pub selectable_only: bool, pub within: Option<EditHost> }
impl LayoutWindow {
    fn text_blocks(&self, dom: DomId, f: BlockFilter) -> impl Iterator<Item = TextBlock>; // layout pre-order, skips walled-off
    fn cmp_blocks(&self, a: TextBlock, b: TextBlock) -> Ordering;
}
```
Used by `set_cross_block_selection` (is_forward + middles), copy/delete sort,
Ctrl+A discovery, `hittest_text_position_global`'s candidate pool,
`resolve_ifc_layout_node`'s fallback, `find_last_*`. Prevents 838adc974's class,
§5 #10, #11, and the NodeId-vs-layout-index split. Cost ≈ 8 sites.

### N7 The choke point: `TextTarget`
```rust
pub struct TextTarget {
    pub block: TextBlock,
    pub host: Option<EditHost>,
    layout_idx: LayoutNodeId,
    layout: Arc<UnifiedLayout>,      // ALWAYS materialized — never the sentinel
    dense: Option<Arc<DenseText>>,
    content_origin: LogicalPosition, // IFC content box, static space
    pub selectable: bool,            // user-select, resolved once
    pub editable: bool,
}
impl TextTarget {
    fn hittest(&self, p: ScrolledContentPoint) -> Option<TextCursor>; // incl. empty-line caret
    fn first_caret(&self) / last_caret(&self) -> CaretPos;          // incl. empty line
    fn content(&self) -> BlockContent;
}
impl LayoutWindow {
    fn text_target(&self, b: TextBlock) -> Option<TextTarget>;
    fn text_target_at_hit(&self, dom: DomId, node: NodeId, hit: &HitTestItem) -> Option<(TextTarget, TextCursor)>;
    fn text_target_at_point(&self, dom: DomId, p: WindowPoint, f: BlockFilter) -> Option<(TextTarget, TextCursor)>;
    fn session_target(&self) -> Option<TextTarget>;
    fn host_target(&self, h: EditHost) -> Option<TextTarget>; // caret-owner first
}
```
All rows of §2.1 collapse into these five constructors; the sentinel/dense/
materialized choice (1cb23d016, eb086526a) and the empty-line fallback (1f71cf82a)
become impossible to forget because only `TextTarget` can hit-test or produce
first/last carets. `focused_cursor_for_point` (12772), which still hand-rolls a
window→local conversion without scroll terms, disappears.

### N8 Unify the selection stores
Replace `multi_cursor` + `cross_block` (+ seat folding) with one
`DocumentSelection { anchor: (TextBlock, TextCursor), focus: (TextBlock, TextCursor),
extra: Vec<(TextBlock, SelectionRange, SelectionOwner)> }`; per-block ranges are
**derived** at paint/copy/delete time from anchor/focus + N6, never cached. A click
sets anchor = focus; any keyboard move/type acts on the one selection. Prevents
§5 #1 and #2's "consumed then dropped", and makes type-to-replace/arrow-collapse
fall out naturally. Largest change; do it after N1/N7.

Process fix: move `SystemChange::SelectAllText`'s body (event.rs:7729-7929) into
`LayoutWindow::select_all(EditHost|None)`; the e2e runner has no port of
SelectAllText, clipboard, or AddCursorAtClick (`runner.rs:1134-1137`), so none of
these paths can be exercised by layout tests or e2e JSON today.

---

## 5. Ranked LIVE bugs (falsifiable; none executed)

Confidence: **C** = certain from the code path, **H** = high (one assumption noted),
**M** = medium.

1. **[C, data loss] A stale cross-block selection outlives click / Tab / arrows /
   typing, and delete/copy use it without checking the target.**
   `cross_block` is cleared only at `window.rs:3849, 19817, 20079`,
   `event.rs:7904`, `text_edit.rs:1715`. `process_mouse_click_for_selection`'s
   success path (19822-19962) and `initialize_editing` (`text_edit.rs:944-973`) do
   not clear it; `apply_selection_op_for_seat` (10363-10457) does not;
   `apply_one_text_changeset` does not. `delete_selection` checks `cross_block`
   first regardless of `target` (20301-20316); paint prefers it
   (`text_edit.rs:1529-1533`).
   Input: drag-select P1→P3 in `div[ce] > p×3`; release; click inside P2 without
   moving; press Backspace. Output: the whole P1..P3 selection is deleted (one
   `ReplaceChildren`), not one character. Same after Tab into another TextInput
   (Backspace there deletes the editor's paragraphs); Left/Right do not collapse the
   highlight; typing inserts at the drag anchor (or replaces mc's stale in-block
   partial range) and leaves the highlight.
   Test: `layout_three_paragraphs` → `set_cross_block_selection(1,…,5,…)` →
   `process_mouse_click_for_selection(point in P2, 0)` (fallback path, no hover
   hit test needed) → assert `get_cross_block_selection().is_none()` (predicted RED).

2. **[C] Deleting/cutting/pasting over a selection whose ends have different
   parents does nothing and loses the selection.** `replace_cross_block_selection`
   takes the selection (3849) then `return None` when `parent != last_parent`
   (3927-3932) — the invariant 838adc974 removed from `set_cross_block_selection`.
   Input: `layout_paragraphs_in_two_boxes`, `set_cross_block_selection(2,…,7,…)`,
   `delete_cross_block_selection()`. Output: `None`, and
   `get_cross_block_selection()` is now `None`. Paste then falls through to inserting
   at the anchor (`event.rs:7629-7720`). The existing
   `deleting_a_document_selection_trims_every_block_it_spans` only covers siblings.

3. **[C/H] Single-node copy (and every B-space consumer) indexes the DOM walk with
   layout cursors.** `get_selected_content_for_clipboard` single-node branch
   (20604-20638) → `extract_clipboard_ranges` indexes `get_text_before_textinput` by
   `source_run`.
   (a) [C] `display: list-item` block "alpha" (marker = run 0, text = run 1 —
   premise asserted by `cross_block_selection.rs:611-614`): drag-select "lph" inside
   it, Copy → `None` (nothing on the clipboard).
   (b) [H — assumes the cluster bytes index the collapsed run text, which is what
   `split_text_for_whitespace` produces] non-`pre` text `"a   b c"`: select "c"
   (layout bytes 4..5 of `"a b c"`), Copy → `"b"` (raw bytes 4..5). Any XML/XHTML
   source text with indentation newlines is affected.
   Same mechanism: `document_selection_spans`/`document_caret` (2754, 2796) report
   wrong bytes; `replace_cross_block_selection` on list items keeps li 1 whole and
   drops li 2's tail (3867-3909); Ctrl+D (13339); seat copy (20464).

4. **[H] Typing / Backspace in a contenteditable list item after a click is a
   no-op (debug builds panic).** Click → session on the `li` with run 1;
   `apply_one_text_changeset` → `caret_text_target` = li → content
   `[Text("alpha")]` → `insert_text` finds no `content[1]` (edit.rs:550) → `NoOp` →
   `debug_assert!(false, …)` at window.rs:16769. `delete_selection` likewise NoOps.
   Test: `div[ce] > ul > li("alpha")`, `process_mouse_click_for_selection` on "alpha",
   `record_text_input("x")` + `apply_text_changeset()` → text unchanged.

5. **[H] A keyboard-opened session paints no selection highlight.** Focus path keys
   the session on the text leaf (`find_last_text_child`, 9690-9693 → 10146-10151);
   `build_primary_text_selections_map` keys `affected_nodes` by it
   (`text_edit.rs:1568`); `paint_selections` looks up the IFC root's own id exactly
   (`display_list.rs:4163`) — while `selection_recolour_for_ifc` resolves ownership
   (4310-4313).
   Input: TextInput (`container[ce,tabindex] > p.value > "hello"`), Tab-focus
   (`handle_focus_change_for_cursor_blink` + `finalize_pending_focus_changes`),
   then Shift+Left (`apply_selection_op` Extend) or Ctrl+A (single block →
   `mc.set_single_range`). Output: mc holds a Range, Copy works, **0
   `SelectionRect`** in the display list (text may be recoloured in the
   `::selection` colour with no background).

6. **[C] Blank lines, remaining halves of 1f71cf82a.** (a) Ctrl+A where the first or
   last block is an empty line (e.g. right after Enter at the end of the document):
   `get_first/last_cluster_cursor()` on the empty strut layout → `None` →
   `DoNothing` (`event.rs:7849-7866`). (b) The single-block fallback uses the
   **sparse** accessor (`event.rs:7894`) → sentinel → always `None` under the dense
   default. (c) A drag that starts on a blank line, or a backward drag that ends on
   one: `set_cross_block_selection` needs `node_text_end_cursor(first)` (3752-3754)
   → `None` → `false` → the drag stays collapsed.

7. **[H] Edits keyed to an inline element.** `caret_text_target` returns the
   nearest non-text node with *any* layout node; inline elements have one
   (`process_block_children` → `process_node` for every inline child,
   layout_tree.rs:2468-2477). Input: programmatic/Tab focus into
   `div[ce] > p > ["Hello ", <b>"world"</b>]` (session = leaf "world", cursor =
   p's last cluster, run 1), type "x". Output: content of `<b>` = 1 run, insert at
   run 1 → NoOp (debug_assert). Backspace likewise. Paint/geometry resolve `p`;
   the commit resolves `b`.

8. **[C] Block-boundary detection ignores affinity, ranges and the IFC.**
   `build_editing_query_state_for_seat` (3015-3040): `at_start` = `run==0 &&
   byte==0` (true for Trailing@0 = after the first glyph); `at_end` compares
   `start_byte_in_run >= len` (false for the layout's Trailing@last); both read the
   **focused host's** flattened content; only the focus end of a range is examined.
   Outputs: click the right half of P2's first glyph, Backspace →
   `MergeWithPrevious` (paragraphs join, glyph survives); select P2 backward to its
   start, Backspace → merge instead of deleting the selection; click at the end of
   P1 (or press End), Delete → never merges with P2.

9. **[C] IME/a11y offset converters disagree** (§2.2). (a) `byte_offset_of_cursor`
   skips `LineBreak`: TextArea `"ab\ncd"`, caret at end → `focused_caret_byte_offset()
   == 4`, while `ime_document()`'s text has length 5 (macOS `selectedRange` vs its
   string). (b) `ime_surrounding_text` (17337) and Wayland ignore affinity: Tab into
   `"42"` (Trailing@1) → Android offset 1, not 2. (c) `byte_offset_to_cursor(0)` →
   Trailing on the first cluster → `set_focused_selection_from_byte_range(0,0)` then
   `focused_caret_byte_offset()` = 1. (d) Wayland sends a range as
   (cursor=start=anchor, anchor=end=focus) — swapped (wayland/mod.rs:10628-10630).
   (e) a11y `SetTextSelection` collapses to the start and writes into whatever
   session exists (16445-16458).

10. **[H] Text in an anonymous IFC is unselectable.** Mixed inline+block children
    create an `InlineWrapper` with `dom_node_id: None` that owns the IFC
    (layout_tree.rs:2668-2672, 2709). The click path needs the root's dom id
    (19620-19625), the fallback (19708) and `hittest_text_position_global` (20119)
    skip it, `ifc_roots_in_document_order` (4052) drops it, `paint_selections`
    returns (display_list.rs:4120). Input: `li > ["Item", ul > li("sub")]`, click on
    "Item" → no caret/selection; a document selection across it neither paints nor
    copies "Item".

11. **[C] Drag target & document order ignore `user-select` and host scope.**
    `hittest_text_position_global` (20118-20163) and `ifc_roots_in_document_order`
    have no `is_text_selectable` / editing-host filter; `paint_selections` does
    (4155). Input: `p("one")`, button label with `user-select:none`, `p("two")`;
    select one→two. Output: copy contains the label text, no highlight over it;
    a drag can end inside widget chrome (f807d2523's labels) or run from a
    TextInput into page text.

12. **[M] Degenerate ranges from structural equality.** Drag start = Trailing@k-1,
    1 px jitter across the glyph edge = Leading@k → `anchor != focus` (20084) →
    `Range` of zero width → `delete_range` deletes nothing (edit.rs:403) → Backspace
    and Delete are NoOps until the next click.

13. **[C] App-API node mismatches and sentinel reads.** `SetSelectAllRange{target}`
    ignores `target` (event.rs:6641-6648); `AddCursor/AddSelectionRange{node_id}`
    ignore the node when a session exists (6651-6700; duplicated in
    `e2e/runner.rs:2790-2840`); `MoveCursorToDocumentStart/End` read the sparse
    layout (6567, 6593) and `CallbackInfo::get_inline_layout_for_node` is own-only
    sparse (callbacks.rs:3264-3286) → `inspect_move_cursor_*` and document jumps are
    no-ops under the dense default.

14. **[M] Resume-point caret in the wrong run.** `restore_caret_from_resume_point`
    puts the session on a text child with `(run 0, byte-in-that-child)`
    (4454-4499); for any text child that is not run 0 of the IFC (second text of a
    rich paragraph, text after `<br>`) the caret is painted and typed into run 0.

15. **[M] Fifth window→IFC conversion.** `focused_cursor_for_point` (12772-12780)
    subtracts the static content origin only — no ancestor or own scroll (cf.
    19290-19307). Android non-cross-block handle drags and `closestPositionToPoint`
    resolve the wrong character in a scrolled field/page.

16. **[latent] Two order definitions in one function.** `set_cross_block_selection`
    takes `is_forward` from NodeId order (3722) and middles from layout-index order
    (3737-3742); if they ever disagree (split-preview parts are appended to the
    tree), `ifc_roots[i_first + 1..i_last]` is a reversed slice and panics.

---

## 6. Suggested order of work

1. Quick guards (each a small, RED-first commit): clear `cross_block` in the click
   success path, `apply_selection_op` Move, text insertion (or replace it), and when
   focus moves to another host (§5 #1); make `replace_cross_block_selection` check
   the parent **before** taking (§5 #2); materialized accessor in the Ctrl+A fallback
   + empty-line cursors for Ctrl+A and the anchor side (§5 #6); key
   `build_primary_text_selections_map` through `text_block_of` (§5 #5).
2. N1 `TextBlock` + N7 `TextTarget` (the choke point) — removes §5 #5, #7, #10, #14,
   #15 and most of §2.1.
3. N3 single content model — removes §5 #3, #4 and the e1b0099e3 class.
4. N2 `EditHost`, N4 `CaretPos`, N5 offsets — §5 #8, #9, #12.
5. N6 + N8 (one document order, one selection store); move SelectAllText into the
   layout crate so the e2e runner and layout tests reach it.
