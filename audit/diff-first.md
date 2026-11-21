# diff-first: the rebuild pipeline that diffs before it styles

Task of the 2026-11-21 audit (RULES.md 8.8). Blind: nothing here was compiled or run.
Branch `audit/diff-first`, cut from the tip of `audit/A3-window` (`4d7cabccc`), `audit/integration` merged once
(`21ff69a5e`, after diff-first-02). The trace this builds on is `audit/A3-window.md`, "Why a full DOM rebuild is
slow" (the 27 stages) and "The diff-first pipeline" (step 1, `dd25f38e7`). `layout.rs` =
`dll/src/desktop/shell2/common/layout.rs`.

**State in one paragraph (second half, 2026-11-21).** A rebuild that keeps the TREE takes the exits of the first
half (texts, nodes changed where they stand, the cascade over the changed nodes). A rebuild that CHANGES the tree
(a node added, removed, moved; a sheet hung on a node changed) is now reconciled BEFORE it is styled (step `tree`,
diff-first-04a/b): the fresh `Dom` is flattened unstyled, `reconcile_dom` pairs the two arenas where they lie (no
copy of the old one), the states follow the nodes, and ONE cascade runs under them for the nodes that are not what
they were (`StyledDom::cascade_reusing`); the layout is told which nodes are what they were whatever the count. A
VirtualView's document that renders again takes the same route (step `view`, diff-first-11), with a third class of
node: a label whose own `left` / `top` / `transform` moved keeps its rows and has only its compact entries built
again. The full path's defect diff-first-10 is fixed. NOT built, and why, each with its design: the solver lays a
RENUMBERED node out anew (diff-first-06b: a node inserted early in the document still costs every box after it),
the display list is emitted whole when the layout tree's node count changed (diff-first-07), the tags and four
smaller walks are O(n) (diff-first-08), `reconcile_dom` itself is O(n) with its hashes and maps (diff-first-05).

## Claims

Functions this task wrote or restructured:

- `core/src/diff.rs`: `fingerprint_dom` (tiers `subtree_len`, `sheets`), `DomFingerprints` (two fields), new
  `same_tree_changes`, `changed_nodes_are_adoptable`, `adopt_changed_nodes`.
- `core/src/prop_cache.rs`: `FlatVecVec` (`replace_rows`, field `orphaned`), `InheritedValues` (`remove_nodes`,
  `place_node`), new `CssPropertyCache::recascade_nodes`, `survives_the_prune` (was a closure of
  `prune_compact_normal_props`), `sort_row_keeping_the_last`; `restyle_cascade` (one line: the node counter).
- `core/src/compact.rs`: `build_compact_cache_with_inheritance_debug` is a wrapper of the new `build_compact_rows(..,
  only)`; new `rebuild_compact_rows`.
- `core/src/styled_dom.rs`: new `StyledDom::nodes_reached_by_changes`, `restyle_changed_nodes`,
  `nodes_cascaded_on_this_thread` / `count_nodes_cascaded`.
- `core/src/generated_content.rs`: new `css_generates_nodes`, `node_generates_nodes`.
- `layout/src/window.rs`: `diff_first_enabled` (now any step), new `DiffFirstStep`, `diff_first_step`,
  `diff_first_steps_of`, `set_diff_first_steps_on_this_thread`, `kept_dom_rebuilds_on_this_thread`,
  `styled_dom_declares_an_animation`, `css_diff_of_kept_dom`, `LayoutWindow::rebuild_can_keep_the_styled_dom`,
  `LayoutWindow::restyle_rebuilt_nodes`.
- `layout.rs`: `regenerate_layout`, exit 1.46 (new), the gate of exit 1.45.
- Tests: `core/src/diff_test.rs` and `core/src/styled_dom_test.rs` (`mod audit_diff_first_tests` at the end of
  each), `dll/src/desktop/shell2/headless/mod.rs` `mod tests` (the `df_*` helpers and tests, after the `a3w_*` ones),
  one literal in `layout/src/audit_tests/a9.rs`.

Second half:
- `core/src/styled_dom.rs`: `create_from_compact_dom` split into `unstyled_from_compact_dom` + `cascade_at_rest`;
  new `unstyled_from_prepared_dom`, `unstyled_from_dom_inheriting`, `cascade_reusing`, `own_style_moved_in_place`,
  `a_raised_state_hands_values_down`, `let_raised_states_in`.
- `core/src/prop_cache.rs`: new `FlatVecVec::gathered`, `InheritedValues::remapped`, `CssPropertyCache::spliced`;
  `recascade_nodes` (the left-state mask of its sheet; the parameter `moved_in_place`).
- `core/src/style.rs`: new `has_structural_pseudo`.
- `layout/src/window.rs`: `DiffFirstStep::{Tree, View}`; `style_user_dom_inheriting` split into
  `flatten_user_dom_inheriting` + `cascade_at_rest`; new `flatten_user_dom_for`,
  `begin_reconciliation_before_styling`, `style_reconciled_dom`, `css_diff_between_knowing`,
  `nodes_clean_for_layout`, `ChildCascade`, `tree_rebuilds_that_kept_rows_on_this_thread`; `unchanged_cascades`
  (parameter `known_unchanged`); `reconcile_rematerialized_child` and its call in
  `invoke_virtual_view_callback_impl`.
- `layout.rs`: `regenerate_layout` exit 1.47 (the style block's `None` arm, the reconciliation call, the states
  call, the GRANULAR DIFF block), `apply_runtime_states_before_layout`, step 3.4's condition.
- Tests: core `styled_dom_test::audit_diff_first_tests` (the `tree_*` helpers and tests), dll `headless/mod.rs`
  (`df_*` extended, `df10_*`, `dfv_*`).

Not touched (the parallel A3-window agent's): `node_patch.rs`, `apply_node_style_change`, `tick_animations`.
`begin_reconciliation` is NOT changed (one argument added to its call of `unchanged_cascades`): it is the
reference path of every equality test.

## The pipeline as built

Stage numbers are those of the trace (`audit/A3-window.md`). n = 4,000 nodes; k = changed nodes; r = nodes the
changes reach (`StyledDom::nodes_reached_by_changes`: a changed node and its descendants; under a sheet with `+` /
`~`, the subtree of its parent). Times are the trace's ESTIMATES (ns per node), not measurements.

| stage | identical rebuild (exit A) | texts changed (exit 1.45) | nodes changed, same tree (exit 1.46) | tree changed (full path) |
|---|---|---|---|---|
| 2 the app builds the `Dom` | n x 300-1,000 | same | same | same |
| 3 form controls | n x 5-10 | same | same | same (twice) |
| 4 `fingerprint_dom` | n x 150-400 (+ ~10 for the two new tiers) | same | same | same |
| 5 the decision | 2 compares | `texts_alone_changed`: n x 2 | `same_tree_changes`: n x 3, then k node checks (`nodes_at`: one walk of the `Dom`, n x 5) | all of the above fail |
| 6A payloads | handlers only | same | same + k node writes | - |
| 7 / 14 states | n x 20; no cascade | same | same; a state that came with the rebuild: the whole cascade | n x 20 + a 2nd cascade under a raised left-of-subject state |
| 9 drop of the `Dom` | n x 100-300 | same | same | moved into the arena |
| 10 styling | 0 | 0 | `recascade_nodes`: r nodes x 2,000-6,000 + the ancestor chains' inherited rows (depth x ~500) + O(n) remnants (see diff-first-08) | n x 2,000-6,000 |
| 12 reconciliation | 0 | 0 | 0 (the CSS diff for r nodes: `css_diff_of_kept_dom`) | n x 1,200-2,500 |
| 15 equivalence walk | 0 | 0 | 0 | n x 50-150 |
| 16 solver hint | - | `dom_diff_clean` | `dom_diff_clean` (every node not reached) | only when the count is unchanged |
| 19-21 layout, list | 0; the list's allocation kept | the text's boxes; list patched | the reached boxes + what reflows; list patched while the tree keeps its shape | every node fingerprinted when the count changed; every item emitted |
| 23 `finish_reconciliation` | 0 | 0 | 0 | n x 100-200 |
| 24 tail walks | 0 | 0 (the exit returns before them) | 0 (gated: same handlers' events, no probe) | n x 10-20 |

The cases, est. total in azul after the app's callback, at 4,000 nodes (ESTIMATES from the work per node, as the
trace's; nothing was measured):
- **identical rebuild**: 2.5-7 ms, unchanged (exit A): the callback's build, the fingerprint and the drop are all
  of it. Count test: `an_identical_rebuild_and_a_changed_text_style_no_node` (0 nodes styled, 0 layout passes, the
  list where it lay).
- **one text or one class changed**: the same 2.5-7 ms + the text's layout (exit 1.45), or + 3-7 styled nodes +
  their layout + about 0.3-0.6 ms of O(n) remnants (exit 1.46, diff-first-08). Was 16-40 ms.
- **500 nodes appearing among 3,500** (exit 1.47, step `tree`): was 25-70 ms. Now, per stage:

| stage | work | est. ns / node | est. ms |
|---|---|---|---|
| 2-4 callback, form controls, fingerprint | n, as every rebuild | 450-1,400 | 1.8-5.6 |
| 10a flatten unstyled (`flatten_user_dom_for`) | n: the walks of A3-window-32, the arena, `cascade_info`, the derived lists | 80-150 | 0.3-0.6 |
| 12a `reconcile_dom` on the arenas where they lie | n: keys and subtree hashes of both sides, three maps; NO arena copy (the record copies the nodes something reads: unmatched ones with handlers, partners of merging nodes) | 600-1,000 | 2.4-4 |
| 12b `transfer_states`, `remap_node_ids` | merging nodes; the managers | - | 0.1-0.3 |
| 14 states onto the unstyled tree | n flags | 20 | 0.1 |
| 10b `cascade_reusing`: which nodes keep their rows | n: five field compares on the two arenas, a state compare | 50-120 | 0.2-0.5 |
| 10c `spliced` | n: offsets, node lists, compact rows gathered | 30-60 | 0.1-0.25 |
| 10d `recascade_nodes` | the 500 nodes that came (+ the ancestor chains' rows) | 2,000-6,000 x 500 | 1-3 |
| 10e tags, `:backdrop`, compact clone | n (diff-first-08) | 60-120 | 0.25-0.5 |
| 12c CSS diff | the matched nodes that did not keep rows | - | < 0.1 |
| 15 `is_layout_equivalent` | counts differ: O(1) | - | 0 |
| 19 solver reconcile | fingerprint only for nodes that are not flagged; a node whose id changed is built fresh (diff-first-06b) | 200-500 x (new + renumbered) | 0.1-2 |
| 20 layout | the new boxes + what reflows; + every renumbered box | - | 2-10 (more when the 500 come early in the document) |
| 21 display list | every node (diff-first-07) | 1,000-3,000 | 4-12 |
| 23-24 `finish_reconciliation`, tail walks | n, as before | 110-220 | 0.45-0.9 |

  Sum: about 13-40 ms, of which the cascade is 1-3 (was 8-24, twice under a hovered widget theme) and the
  reconciliation 2.5-4.5 (was 5-10); what is left is the display list (07), the layout of renumbered boxes (06b)
  and the callback's own build (09). Count tests: `rows_that_come_are_styled_and_laid_out_and_no_other_node`,
  `five_hundred_nodes_among_3500_are_styled_alone` (501 nodes cascaded).
- **a view's settle, 12.5k labels (25k nodes), all moved, 5 % exchanged** (step `view`): the cascade ran for
  25k nodes (50-150 ms by the same per-node figures); now for the 1.25k nodes that entered (2.5-7.5 ms) + compact
  rows for the 11.9k labels that moved (about 1 us each: 12 ms; the builder's row for one node) + the O(n) parts
  above (flatten, `reconcile_dom` by key, the keep test, the splice: 20-35 ms at 25k nodes). The solver keeps the
  interior of every label that kept its node id and places it again; an exchange that shifts ids lays out the
  labels after it (06b). A pan that exchanges nothing: 0 nodes cascaded, 0 boxes built
  (`a_view_whose_labels_all_moved_styles_no_node_and_keeps_every_box`).

## What "unchanged" means

The early diff is `fingerprint_dom` (one pre-order walk of the fresh `Dom`, before anything is styled) against the
fingerprints of the build before (`LayoutWindow::last_dom_fingerprints`: 28 bytes per node, no copy of the DOM).
Per node: `structure` (every COMPARED row of the node-field table `layout_inputs::NodeField` - type and text,
handlers' events, flags, accessibility, Fluent arguments, attributes with ids and classes, SVG data, presence of
menus, key, part-of-parent - and the child count), `style` (the inline style and the sheets attached to the node),
`subtree_len`; for texts `text_shapes`; for nodes with sheets `sheets`.
- The TREE is unchanged when `subtree_len` is equal (a pre-order with every subtree size has one shape).
- The SHEETS are unchanged when `sheets` is equal (each attached sheet, and the node it hangs on).
- A NODE is unchanged when its `structure` and `style` are equal. Callbacks' identity, datasets, a view's callback,
  markers are CARRIED, not compared (the table says which): every exit that keeps the styled DOM writes the fresh
  ones onto it (`adopt_fresh_payloads`).
- Sibling position needs no tier of its own while the tree is unchanged: `CascadeInfo` (index among element
  siblings, last element child) is a function of the tree and of which nodes are texts, and a changed node keeps
  its type (`changed_nodes_are_adoptable`).
- IDENTITY (which old node a new node is, for focus, scroll, hover, text-edit state, datasets): in the same tree a
  node whose type, terminal key and - where `NodeIdentityConfig` says so - classes are what they were has the
  reconciliation key it had, and is the node at its place. This is the pairing the pre-cascade exit has always
  used for an identical rebuild and step 1 for texts. **Where the full path differs**: `reconcile_dom`'s pass A2
  pairs by CONTENT before place - a subtree whose `NodeData::hash` content stood at another place in the build
  before, unique on both sides, takes that place's identity (two rows that exchange their texts are, to the full
  path, two rows that swapped). The kept-DOM exits keep identity with the place. No fingerprint can reproduce A2
  exactly (`NodeData::hash` reads other fields than the fingerprint tiers: handler function pointers, dataset
  allocations, inline properties by type only), so this is a stated difference, not a gate. It shows only as
  state following content instead of place, and not at all without such a state (`animation: move` is gated).

## The unit of styling reuse, and its proof

Unit: **a node none of whose ancestors-or-self is named dirty**, in a DOM whose tree, sheets, window context and
node states are what they were when its rows were made. `recascade_nodes` re-styles the named nodes; the proof that
an unnamed node's rows stand is on that function and repeated here with the cases the task asked about:
- matched rules: `matches_html_element_remembering` reads the subject, then walks LEFT: ancestors (` `, `>`),
  the previous element sibling (`+`), every previous element sibling (`~`). Never a descendant, never a following
  sibling. Per node it reads the type's path, classes, ids, attributes, the anonymous / text / generated flags, the
  node's index against a scope (`Root(range)`: the subtree a sheet is attached to), `index == 0` for `:root`, and
  `CascadeInfo`. There is no `:empty`, `:has`, `:not`, `:nth-last-child`, `:only-child` in this engine
  (`CssPathPseudoSelector`): the child count is read by no selector.
- **a state rule left of a subject** (`.btn:hover .ink`): the rule holds by the state of the anchor when the
  cascade ran. On the kept-DOM exits the kept rows were cascaded under the states the nodes hold (A3-window-46's
  invariant) and the named nodes are cascaded under the same states. When a state CAME with the rebuild
  (`raise_runtime_states` changed a flag), the exit runs the whole cascade (`StyledDom::restyle`): `style` is not
  used for that rebuild.
- **`:nth-child`, `:first`, `:last`**: read `CascadeInfo`, unchanged in an unchanged tree whose nodes keep their
  types. (For a changed tree: see diff-first-04, condition (b).)
- **sibling combinators**: a changed node can switch a rule for the element siblings AFTER it (and their
  descendants). `nodes_reached_by_changes` names the whole subtree of the changed node's parent when any rule of
  the retained sheet has `+` or `~` (one scan of the rules per rebuild), which usually is more than half of a flat
  page: then `recascade_nodes` says no and the whole cascade runs. A sharper reach (only the later siblings; only
  when the changed node's classes are named left of a sibling combinator) is possible and not built.
- **inherited properties**: flow parent to child through three tables. (1) `cascaded_props`: a child copies the
  parent's inheritable declarations of every state; (2) `computed_values`: the child's values from the parent's
  values, plus the parent's state overlay (what a hovered parent hands down: `state_inherited_values`); (3) the
  compact entries: the child copies the parent's inheritable slots. All three are recomputed for a named node from
  its parent's CURRENT rows; a named node's descendants are named. One table of an UNNAMED node is needed that the
  cache no longer holds: the parent's inherited row before the prune (`prune_compact_normal_props` drops the
  resting compact-encoded values from the kept rows). `recascade_nodes` rebuilds those rows for the ancestor
  chains of the named nodes, from the root down, uses them and drops them. (This is the main thing the
  predecessor's design had not seen: its `adopt_rows` copied "the row of every per-node table", and the kept rows
  are not what a child inherits from.)
- **UA defaults**: a function of the node's type and of what it declares; the stale copies below an element whose
  UA default shadowed an inherited value (`hand_ua_defaults_down`) are followed along the same ancestor chains.
- **custom properties**: resolved by a DOM-wide pass; `recascade_nodes` says no when the sheet or any node has one.

Equality guard: `core` `the_cascade_over_changed_nodes_answers_what_a_new_cascade_answers` (and `_under_a_hover`,
`_beside_sibling_rules`): after every random change, the kept DOM's matched rows, inherited rows, computed values,
compact entries and all ~200 properties per node in three states equal a DOM styled from scratch.

## The five diffs: which remain

1. `fingerprint_dom` (early): THE diff for every rebuild that keeps the tree. Remains.
2. `reconcile_dom`: not run on the three kept-DOM exits. Remains on the full path (a changed tree), unchanged: it is
   the only definition of identity for a changed tree (its passes A2 / A3 follow content and children).
3. the CSS diff: on exit 1.46 it runs for the r reached nodes (`css_diff_of_kept_dom`), fed by the early diff; on
   the full path as before.
4. `is_layout_equivalent`: not run on the kept-DOM exits. Remains on the full path as the safety net for a DOM whose
   fingerprints moved while it reads alike.
5. the solver's reconcile (`NodeDataFingerprint` per node): skipped for every node the early diff calls clean
   (`dom_diff_clean`), on all kept-DOM exits. It stays for the reached nodes - it classifies how far a change
   reaches into the layout (insets / paint / full), which the early diff does not know.
6. (the renderers' list diff: A1 / A4's.)

## Findings

### diff-first-01 the early diff could only say "alike", "texts", or "differs"
- file:line: `core/src/diff.rs` `fingerprint_dom`, `DomFingerprints`
- class: waste (enabler)
- fix: tiers `subtree_len` and `sheets`; `same_tree_changes`, `changed_nodes_are_adoptable`, `adopt_changed_nodes`.
- test: `diff_test::audit_diff_first_tests` (six tests, named in the commit).
- severity: high
- status: fixed 3d51fc7fa

### diff-first-02 a rebuild that changed nodes where they stand took the full path
- file:line: `layout.rs` `regenerate_layout` (exit 1.46); `layout/src/window.rs` `restyle_rebuilt_nodes`,
  `css_diff_of_kept_dom`, `rebuild_can_keep_the_styled_dom`; `core/src/styled_dom.rs` `nodes_reached_by_changes`
- class: waste, per `RefreshDom` that toggles a class, moves a label, changes an id: stages 10-15, 23 for all n nodes
- fix: the exit keeps the styled DOM, writes the changed nodes, restyles in place, stages the CSS diff of the
  reached nodes, hands the solver the clean flags. Gates (each is something a full rebuild does that this exit does
  not): the window's size or theme moved with the rebuild; a changed node is another node by the app's identity
  rules, has an `Updated` handler, other handlers' events, a dataset that came or went, is an image, icon, view,
  geolocation probe, localizable or translated text, carries a presentational hint or generated content; any node
  declares an `animation`; the sheet makes generated content; the app theme is a flora spin; an E2E mount; CSD
  injection (node counts differ).
- test: dll `a_rebuild_that_changed_nodes_leaves_what_the_full_path_leaves`, `.._under_the_pointer_..`,
  `one_changed_class_keeps_the_styled_dom_and_lays_out_its_own_boxes`, `the_steps_of_the_switch_are_named_one_by_one`.
- severity: high
- status: fixed 03af4217d (and 5acd151cf: the derived node lists, A6's rule)

### diff-first-03 the cascade of a kept DOM ran for every node
- file:line: `core/src/prop_cache.rs` `recascade_nodes`; `core/src/compact.rs` `build_compact_rows`;
  `core/src/styled_dom.rs` `restyle_changed_nodes`
- class: waste, per rebuild of diff-first-02: n x 2,000-6,000 ns for r changed nodes
- fix: see "The unit of styling reuse". Fallback to the whole cascade is built in (`recascade_nodes` answers false).
- test: core `styled_dom_test::audit_diff_first_tests` (six tests), dll
  `a_rebuild_that_styled_its_changed_nodes_leaves_what_the_full_path_leaves`, `.._under_the_pointer_..`,
  `one_changed_class_styles_its_row_and_no_other_node`, `an_identical_rebuild_and_a_changed_text_style_no_node`.
- severity: high
- status: fixed 009e311d8 (count test for the cheapest rebuilds: 00b4d8305)

### diff-first-04 styling reuse when the TREE changed (500 nodes appear among 3,500)
- file:line: `core/src/styled_dom.rs` `cascade_reusing`, `unstyled_from_compact_dom`, `cascade_at_rest`;
  `core/src/prop_cache.rs` `spliced`, `FlatVecVec::gathered`, `InheritedValues::remapped`; `layout/src/window.rs`
  `begin_reconciliation_before_styling`, `style_reconciled_dom`; `layout.rs` exit 1.47
- class: waste, per rebuild that adds, removes or moves a node: stage 10 for all n nodes, twice under a state
- what is built (it differs from the design this block held before in three places, each forced by the code):
  1. THE PAIRING is `reconcile_dom` itself, run on the two flat arenas BEFORE anything is styled. It reads node
     data and hierarchy only; the new arena it sees is the one the full path gives it (generated nodes in, hints
     applied: `unstyled_from_compact_dom` is the first half of the creation), so the pairing IS the full path's:
     no proof of agreement is owed, and identity (focus, hover, scroll, datasets) follows it as before. A second
     pairing over fingerprints (the earlier design's `own` / `subtree_hash` tiers) is not built: any pairing
     serves the styling as long as each pair is CHECKED, and the reconciliation has to run anyway.
  2. WHICH NODES KEEP THEIR ROWS is decided per pair, on the two arenas (not on fingerprints of an earlier
     build: a callback may have written the old arena since - `set_node_style` - and the fingerprints know
     nothing of it): the six conditions and the proof are on `cascade_reusing`. One input is NOT in the arena:
     the sheets hung on a node (`Dom::with_css` is a sheet scoped to its node, not a field of the node; the
     flat arena holds one merged sheet). They come from `DomFingerprints::sheets` of the two builds, where each
     build's fingerprints number the nodes as its arena does (the shell checks the counts; otherwise nothing is
     kept).
  3. THE SPLICE re-indexes the tables as they lie (`spliced`), then `recascade_nodes` runs for the rest. It now
     sets `left_state_mask` for the sheet it is given (a spliced cache meets another sheet).
  4. THE STATES: `begin_reconciliation` is NOT split in place (it stays the reference); its style-free half is
     a function of its own (`begin_reconciliation_before_styling`: reconcile, mount / unmount, transfer, remap)
     and the half that reads both cascades another (`style_reconciled_dom`: cascade, CSS diff, override
     migration, staged dirt). Between them the shell raises the states (`raise_runtime_states`): the cascade
     runs once under them. Where the reconciliation needs both cascades it says no BEFORE it changes anything
     and the shell takes the full path: an `animation` declared on the old tree (transitions and moves are
     measured from the old cascade under the old interaction state), a departing node with
     `-azul-animation-out` (the old result becomes a zombie), another selector context.
- where no row can be kept (`cascade_reusing` answers `None`: more than half of the nodes changed, a custom
  property, a generated node, an inactive window, a drag state, another `*` bucket), the tree is cascaded at rest
  and the states let in afterwards: the full path's styling, after the reconciliation instead of before it.
- a stated difference from the full path: the CSS diff reads the new cascade under the real states (the full
  path reads it at rest, then cascades again), so a node below a hovered `.btn:hover .ink` anchor is NOT named
  restyled on every rebuild. The layout result is the same; fewer boxes are marked.
- known limit of the pairing for styling: `reconcile_dom` pairs twins (equal subtrees in one container) by place,
  so after an insertion among keyless siblings the twin DESCENDANTS of the shifted siblings are paired with the
  neighbour's and fail condition 2 (their rows are made again; correct, not minimal). A second candidate (the
  child at the same index under the parent's partner) would keep them; not built.
- test: core `a_cascade_that_keeps_rows_across_a_changed_tree_answers_what_a_new_cascade_answers` (+ `_under_a_hover`,
  `_beside_sibling_rules`, `_under_structural_rules`), `sections_that_come_are_cascaded_and_no_other_node`,
  `a_node_written_since_its_build_does_not_hand_on_its_rows`, `a_node_whose_own_sheet_changed_is_cascaded_again`,
  `an_unstyled_dom_cascaded_at_rest_is_the_dom_the_creation_makes`; dll
  `a_rebuild_that_changed_the_tree_leaves_what_the_full_path_leaves` (+ `_under_the_pointer_`),
  `rows_that_come_are_styled_and_laid_out_and_no_other_node`, `five_hundred_nodes_among_3500_are_styled_alone`.
- severity: high
- status: fixed e332a6b34 (core: 06480341b)

### diff-first-05 the copies and maps of the reconciliation
- file:line: `layout/src/window.rs` `begin_reconciliation` (the copy at `reconcile_arena_copy`),
  `begin_reconciliation_before_styling`; `core/src/diff.rs` `reconcile_dom`
- class: waste, per rebuild that changes the tree: n x 1,200-2,500 ns
- what is done: on the three kept-DOM exits none of it runs. On exit 1.47 (step `tree`) the old arena and both
  hierarchies are READ WHERE THEY LIE: `PendingReconciliation::old_node_data` holds a real node only where
  something reads it afterwards (an unmatched old node with handlers: `BeforeUnmount`, `FocusLost`; the old
  partner of a node with a merge callback or of a placeholder image: `transfer_states`) and an empty node
  elsewhere; the repoint pass visits the copied nodes only. The four sets of n ids were gone already
  (A3-window-14). `begin_reconciliation` itself (the full path) keeps its copy.
- what is left: `reconcile_dom`'s own O(n) - the reconciliation keys and the subtree hashes of BOTH sides (a
  `NodeData::hash` per node per side), `old_by_rec_key` and `old_by_subtree` (a `VecDeque` per distinct key).
  Keeping the old side's hashes from the last rebuild is NOT sound as it stands: every exit that keeps the
  styled DOM writes payloads onto the old arena (`adopt_fresh_payloads`: handlers, datasets), and
  `NodeData::hash` reads handler pointers and dataset allocations, so the kept hashes would be stale after any
  identical rebuild. It needs a hash of the COMPARED fields only (the fingerprint's `structure` without the
  child count) in passes A2 / B2, which changes what "the same content" means for identity: a decision, not a
  local fix. The two maps as one sorted `Vec<(key, node)>` each is local and not made.
- severity: medium
- status: open: the arena copy is gone on the step `tree` (e332a6b34); the hashes and maps of `reconcile_dom` remain (est. 2.4-4 ms at 4,000 nodes), with the reason the cached-hash shortcut is unsound

### diff-first-06 the solver fingerprints every node when the node count changed
- file:line: `layout.rs` "GRANULAR DIFF"; `layout/src/window.rs` `nodes_clean_for_layout`;
  `layout/src/solver3/cache.rs` `reconcile_node` (read in full for this: `DomDiff`, `diff_clean_here`, `renumbered`)
- class: waste, per rebuild that changes the tree: n x 200-500 ns
- fix: the solver's reader already takes `dom_diff_clean` per NEW node id beside `dom_pairing` and uses a flag
  only to take over the fingerprint of the node's old box after a state-hash check; nothing in it needs equal
  counts. The shell now sets it on the step `tree` (and the view's pass on the step `view`) from the
  reconciliation: a node is flagged when it is unchanged for the cascade, paired with the old node OF ITS OWN
  INDEX, and alike in every compared field (`NodeField::reads_alike` over the whole table). No child-count
  condition is needed (the fingerprint holds none); the whole-pass skip (`reconcile_skipped_unchanged`) cannot
  fire wrongly: a node that came is never flagged, a node that went with a box leaves `dom_to_layout` with an id
  past the new count, and one that went without a box changes no tree.
- test: dll `rows_that_come_are_styled_and_laid_out_and_no_other_node` (fresh boxes do not grow with the page).
- severity: medium
- status: fixed e332a6b34

### diff-first-06b the solver lays a renumbered node out anew
- file:line: `layout/src/solver3/cache.rs` `reconcile_node` (`renumbered`: `diff.pairing.is_some() &&
  !diff.keeps(old.dom_node_id, new_dom_id)` -> `DirtyFlag::Layout`), `DomDiff::keeps`
- class: waste, per rebuild that inserts or removes a node: every node AFTER it in document order gets another
  id, is built fresh and laid out again (boxes, intrinsic sizes, inline layouts; the shaped runs come from the
  shaping cache). 500 nodes appended at the end of a page cost 500 boxes; inserted at its top, all 4,000. A
  view whose labels enter and leave in the middle of its list lays out every label after the first of them.
- why it is so (the comment on `keeps`): what was laid out for a box names nodes by their OLD ids - the inline
  items of its inline layout, its run's content - so the box cannot be carried over under another id as it is.
- design (A3-solver's and A8's files; not read by this session beyond `cache.rs` 2050-2440 and 3290-3700):
  carry the box and REWRITE the ids it holds through `DomDiff::now`: (1) inventory every `NodeId` inside
  `LayoutNode` (hot / warm / cold) and inside the cached inline layout (`text3::cache`: the items' source
  nodes, the content index of a run); (2) `adopt_node_from_old_renumbered(old, new_parent, new_dom_id, &diff)`
  maps them, and refuses (builds fresh) when one of them has no new id; (3) the display list's patch then
  needs 07. Until then the flag of 06 is not set for a renumbered node (it would gain nothing).
- severity: high (it is what is left of "unchanged subtrees keep their boxes" when a node comes early)
- status: blocked (a): the cached inline layouts name DOM nodes by id (text3, A8's and A3-solver's files); the design is this block; built short of it: the flags, and every node that keeps its id keeps its box

### diff-first-07 the display list is emitted for every node when one node comes or goes
- file:line: `layout/src/solver3/display_list.rs` `PatchState` (5724; refused when
  `last_reconcile_structure_preserved` is false: `solver3/mod.rs` 989, 2192, 2324)
- class: waste, per rebuild that changes the layout tree's node count: every item emitted, a new list for both
  renderers (est. 4-12 ms at 4,000 nodes)
- design (read for this session: `PatchState`'s fields and the gate; not `PatchState::build` nor the walk): the
  patch keys the runs of the previous list by LAYOUT index and takes position deltas and previous sizes by the
  same index, so it needs the same indices. Across a changed tree it needs (1) from the reconcile: per new
  layout node the old layout index it was adopted from (`reconcile_node` knows it: `old_tree_idx` of a reused
  node; `None` for a fresh one), kept in `ReconciliationResult`; (2) `PatchState::build` reading runs, deltas
  and previous sizes through that map, with every fresh node in `reemit`; (3) the three gates accepting "mapped"
  beside "structure preserved". A reused node keeps its DOM id (06b), so the ids inside its spliced items stand.
  It does not help renumbered nodes until 06b exists.
- severity: high
- status: blocked (a): the old-to-new layout index map through `PatchState::build` and the walk that consumes the runs (display_list.rs, 20k lines, A3-core's; not read in full by any session); the design is this block

### diff-first-08 what is still O(n) on the kept-DOM exit for changed nodes
- file:line: `layout/src/window.rs` `restyle_rebuilt_nodes`, `rebuild_can_keep_the_styled_dom`;
  `core/src/prop_cache.rs` `recascade_nodes`; `core/src/styled_dom.rs` `restyle_changed_nodes`
- class: waste (linear, small constants), per rebuild on exit 1.46; together est. 0.3-0.6 ms at 4,000 nodes
- the walks, each with its cost and what removes it:
  1. `old_cache.clone()` (the cascade before, for the CSS diff): the flat stores' offsets, the inherited node
     lists, the four compact row vectors (about 190 bytes per node): n x ~30 ns. Remove: keep only the old rows of
     the reached nodes (`css_diff_of_kept_dom` reads nothing else of the old cascade but `computed_values` through
     `InheritedIds`, which can take the removed nodes' old values as a side list).
  2. `InheritedIds` tables: built for all nodes of both cascades on first use: n x ~40 ns. Same remedy.
  3. `styled_dom_declares_an_animation`: a declared-only lookup per node: n x 30-60 ns. Remove: a flag on the
     cache, set by the cascade when a node's row holds `animation` (one more bit where `left_state_mask` is made).
  4. `generate_tag_ids`: every node: n x 50-100 ns. Remove: a tag is a function of the node's own rows and its
     children's types; merge the old list with new entries for the reached nodes.
  5. `recascade_nodes`: `vec![Vec::new(); n]` twice, `vec![[0u128; 2]; n]`, `vec![None; n]`, the `:backdrop`
     scan of every node's inline style, `rebuild_compact_rows`' clone of the compact cache and its font-hash
     pass, `sync_backdrop_state`: n x ~40 ns together. Remove: index the scratch rows by position in `rebuilt`.
  6. `nodes_at` (the changed nodes read out of the `Dom` tree: twice): n x 5 ns each.
  7. `rebuild_derived_node_lists` (A6's): n x 4 questions.
- severity: low
- status: open: seven linear walks named above, each with its remedy; none of them is a cascade or a hash of node data, and each remedy is local

### diff-first-09 the `Dom` itself: what building it costs, what `FastDom` saves
- file:line: `core/src/dom.rs` `Dom` (4812), `FastDom`, `convert_dom_into_compact_dom` (`styled_dom.rs`)
- class: waste, per rebuild, the floor of every case above (stages 2, 4, 9: est. 0.6-2.4 us per node)
- what it costs per node through the public builder (by reading): the `NodeData` (inline, about 100 bytes) moved
  into the parent's `Vec<Dom>` (`with_child`: a push; the vector doubles and moves about 250 bytes per child each
  time); `with_class` / `with_id`: `add_class` swaps the attribute vector out and back and pushes (the first one
  allocates the `Box<NodeDataExt>` and the vector, each further one may reallocate it); an inline style
  (`add_css_property`): the same on `style.rules`, one `CssRuleBlock` with a one-element declaration vector per
  property; `with_css("..")`: a parse per node; a handler: a vector and a `RefAny` clone. Then per rebuild the
  tree is walked by `fingerprint_dom` (two SipHashes of the node), by three preparation walks when it is styled,
  and flattened (`convert_dom_into_compact_dom` first fills its arena with n default `NodeData` and overwrites
  them), and on every exit that keeps the styled DOM it is dropped: 4-8 frees per node.
- what `FastDom` saves: the per-parent `Vec<Dom>` and the flatten. Not the per-node boxes (the `NodeDataExt`, the
  attribute and style vectors), and not the walk of the fingerprint. About a third of stage 2's allocations.
- should the public builder produce the flat form: yes, as an ADDITION, and with the fingerprint folded in - that
  is where the gain is, not in the arena. `DomBuilder` (new, `core/src/dom.rs`, api.json class): `open(NodeData)`,
  `close()`, `leaf(NodeData)`, `sheet(Css)`; it appends to `NodeDataVec` / `NodeHierarchyItemVec`, and computes
  per node what `fingerprint_dom` computes (`structure`, `style`, `subtree_len`, `text_shapes`, `sheets`, the
  carried values) as it appends: `close()` knows the subtree's length and the child count. `finish() -> FastDom`
  with the fingerprints inside (`FastDom` gains `fingerprints: Option<DomFingerprints>`), `LayoutCallback` may
  return either (`Dom` converts by one walk, as today). Stages 3, 4 and the flatten then cost nothing for an app
  that builds through it, and stage 9 is two vector frees. The larger win is the predecessor's: a component whose
  inputs are equal hands back a HANDLE (the `subtree_hash` it got from the builder last time) instead of nodes,
  which the builder records as "the old subtree, unchanged" - then stage 2 is O(changed) too, and that handle is
  exactly the `subtree_hash` pair diff-first-04's map consumes without a walk.
- why not built: it is an api.json addition with codegen and a second form through every consumer of the layout
  callback's result (the shells, `resolve_form_controls`, the icon and translation passes, `fingerprint_dom`'s
  callers, the E2E mount); contained it is not.
- severity: medium
- status: blocked (a): a second DOM form through the layout callback and its consumers (api.json addition + codegen); the design is this block

### diff-first-10 the full path leaves the children of a hovered node at their resting inherited values
- file:line: `layout.rs` `apply_runtime_states_before_layout`; `core/src/styled_dom.rs`
  `a_raised_state_hands_values_down`, `let_raised_states_in`
- class: wrong-output (until the next hover change), on the FULL path, found while proving equality
- evidence: a full rebuild cascades the new DOM at rest, then raises the runtime states flag by flag;
  `restyle_for_raised_states` runs the cascade again only when a raised state is named LEFT of a subject. A node
  that is hovered and declares an INHERITED property for `:hover` showed its own hover colour while its children
  kept the resting one, until the pointer left and entered again.
- fix: where no whole cascade ran and a node in a state hands a value down
  (`CssPropertyCache::state_inherited_values` non-empty for a node that is not at rest),
  `recompute_inheritance_for_states()` - what `restyle_states` runs when such a hover arrives by event. The dll
  equality walks (`df_layout`'s sheet) carry an inherited value in their hover rule since.
- not examined further: `recompute_inheritance_for_states` runs `compute_inherited_values` over rows the
  creation PRUNED (the resting compact-encoded values are gone from `cascaded_props`), as the hover path always
  did; for a compact-encoded inheritable type with a UA default on the node (`font-weight` of `<b>`) the
  `computed_values` entry it leaves is the inherited one, while the compact cache (what the layout reads at
  rest) is right. A3-core's table; noted there as a question, not changed here.
- test: dll `a_full_rebuild_keeps_the_hover_colour_the_hovered_row_hands_its_text`.
- severity: medium
- status: fixed f589b3743

### diff-first-11 a view that renders again was styled in full (the editor's settle)
- file:line: `layout/src/window.rs` `invoke_virtual_view_callback_impl`, `reconcile_rematerialized_child`,
  `ChildCascade`; `core/src/styled_dom.rs` `cascade_reusing` (the class "moved in place"),
  `own_style_moved_in_place`; `core/src/prop_cache.rs` `recascade_nodes` (`moved_in_place`)
- class: waste, per re-render of a view: the cascade for every node of its document (12.5k labels = 25k nodes)
- fix: the step `view`: the callback's DOM is flattened unstyled, `reconcile_rematerialized_child` pairs it
  first (by key: pass A1) and then runs the cascade that keeps rows; the CSS diff skips the unchanged nodes; the
  pass gets `dom_diff_clean`. A label whose OWN style moved in place (the values of its insets, of an exact
  `transform` / `opacity`; the same declarations otherwise; no hint, no CSS-wide keyword) keeps its matched and
  inherited rows and computed values and has its compact entries built again: none of the kept tables holds a
  node's own declarations, the properties do not inherit, no selector reads a style. The proof is on
  `recascade_nodes` and `own_style_moved_in_place`.
- NOT built for a view: the kept-DOM exits `text` / `nodes` / `style` (no flatten, no `reconcile_dom`, no
  remap for a re-render that keeps the tree). What they would save beyond this step: the flatten and
  `reconcile_dom` (est. 0.7-1.2 us per node: 17-30 ms at 25k nodes). The ingredients exist
  (`same_tree_changes` on `child_dom_fingerprints`, `adopt_changed_nodes`, `css_diff_of_kept_dom`,
  `relayout_child_dom_in_place` as the template for the pass); a kept view's document would keep the hover
  states of its nodes, which the full path drops (a new document is at rest): the same stated difference as
  identity on the root's exits.
- test: dll `a_view_rendered_again_leaves_what_the_full_path_leaves`,
  `a_view_whose_labels_all_moved_styles_no_node_and_keeps_every_box`,
  `a_view_that_exchanged_5_percent_of_its_labels_styles_5_percent`.
- severity: high
- status: fixed 43bb97f38

### diff-first-12 a lost focus is looked up by node index alone
- file:line: `layout.rs` `regenerate_layout`, "A FOCUSED node that the rebuild did not carry over"
  (`pending.old_node_data.get(idx)` for every entry of `take_focus_lost_to_unmount()`, whatever its `dom`)
- class: wrong-output (a `FocusLost` handler of the wrong node), full path and step `tree` alike
- evidence: an entry whose node is in a view's document (queued by that view's remap) is read against the
  WINDOW's old arena by its index. On the full path the copy holds every node, so the window's node of that
  index hears a `FocusLost` it does not own; on the step `tree` the record holds unmatched nodes only, so it
  usually hears nothing. Neither is right: the handler is on the view's old node.
- fix (not made: the queue is A6's `focus_cursor.rs`, the dispatch the shell's): skip entries whose `dom` is not
  the reconciled one here, and resolve a view's entries in `reconcile_rematerialized_child`, where its old arena
  is at hand (it already resolves `BeforeUnmount` there).
- severity: low
- status: open: the exact change is given; it touches A6's queue contract (which reconciliation drains which DOM's entries)

### A3-window-32 (styled_dom part) the preparation walks of a `Dom` before it is styled
- file:line: `core/src/styled_dom.rs` `create_from_dom_inheriting`, `create_from_prepared_dom`
- what the row said: "the four preparation walks run twice; every attached sheet is cloned". Both are gone on the
  branch this task started from: A3-core-11b (`9e3e2f5be`: both constructors end in `create_from_prepared_dom`) and
  A3-core-09 (`806490941`: `take_css_from_dom` moves the sheets).
- what remains per STYLED DOM (the full path, and a view's render): `fixup_children_estimated` (one walk: the
  subtree counts the flatten needs), `scope_inline_css` (one walk: writes each attached sheet's node range),
  `take_css_from_dom` (one walk), the flatten (one walk + n default nodes written first), est. n x 30-60 ns
  together beside a cascade of n x 2,000-6,000. Scope and take can be one walk (both visit every node to look at
  `dom.css`); the counts could come from the builder (diff-first-09).
- after this task: on the three exits that keep the styled DOM (identical, texts, changed nodes) NONE of these
  walks runs - the `Dom` is fingerprinted, read at the changed nodes (`nodes_at`) and dropped. They run only when
  the tree changed (and for a view's child DOM, whose exits are not built: see "Next").
- severity: medium (as the row), low for what remains
- status: open: three walks per styled DOM remain on the full path (about 1-3 % of its cascade); merging `scope_inline_css` into `take_css_from_dom` is the local remedy, not made

### Checked: fine
- `Css::sort_by_specificity` / `sort_rules_in_cascade_order` run in `recascade_nodes` on the retained sheet: the
  sheet was sorted by the cascade that made the rows, sorting it again does not move a rule (the rule order the
  kept rows were matched in is the order the named nodes are matched in). `recascade_nodes` also compares the `*`
  bucket it derives with the one the cache holds and says no on a difference.
- Tag ids are `node index + 1` (`generate_tag_ids`): a kept DOM keeps every tag of an unreached node by
  construction. (The predecessor's design wanted the tags "kept": they always were.)
- `generate_anonymous_table_elements` is behind a feature `table_layout` that `core/Cargo.toml` does not define: no
  pass adds nodes after the cascade but generated content, which the exits gate.

## Switches

`AZ_DIFF_FIRST` (read once per process; `layout/src/window.rs` `diff_first_steps_of`):
- unset, or any value that names no step (`1`, `on`): every step.
- `0`: no step: every rebuild whose fingerprints moved takes the full path (the reference of every equality test).
- a comma-separated list: exactly the named steps.
  - `text`: exit 1.45 (texts alone). Off: such a rebuild takes the next exit that is on, else the full path.
  - `nodes`: exit 1.46 (nodes changed where they stand; without `style` it runs the whole cascade in place).
  - `style`: inside `nodes`, the cascade over the reached nodes only.
  - `tree`: exit 1.47 (the window's DOM: reconciled before it is styled; rows kept across a changed tree; the
    solver's flags whatever the count). Off: a rebuild that changed the tree takes the full path.
  - `view`: the same for a VirtualView's document that renders again (`reconcile_rematerialized_child`). Off:
    every re-render of a view is styled in full, then reconciled.
  Examples: `AZ_DIFF_FIRST=text,nodes,style` = the state before the second half; `AZ_DIFF_FIRST=tree` = every
  changed rebuild of the window's DOM down exit 1.47; `AZ_DIFF_FIRST=text,nodes,style,tree` = everything but views.
A test picks its steps per thread: `window::set_diff_first_steps_on_this_thread(Some(&[..]))`,
`set_diff_first_on_this_thread(Some(bool))`. Which path a rebuild took: `window::kept_dom_rebuilds_on_this_thread()`
(exit 1.46), `window::tree_rebuilds_that_kept_rows_on_this_thread()` (exit 1.47 and a view's re-render, when rows
were kept), `styled_dom::nodes_cascaded_on_this_thread()`, `styled_dom::compact_cache_rebuilds_on_this_thread()`
(whole cascades). The pre-cascade exit for an identical rebuild is older than the switch and not under it.

## The gates (what sends a rebuild down the full path, or the whole cascade)

Status per gate: KEPT (why it must stay) or NARROWED (what it is now). "Full path" = style at rest, then
`begin_reconciliation`; "whole cascade" = every node styled, inside an exit that otherwise holds.

| gate | where | status |
|---|---|---|
| the window's theme, app theme or rice moved (`theme_rebuild`) | every exit | KEPT: another sheet or UA table over the same nodes; no row stands |
| the decoration mode changed, or decorations are injected | 1.45-1.47 | KEPT: the injected tree is appended to a STYLED tree (`csd.rs`), ids shift |
| window size or selector context moved with the rebuild | 1.46: full path; 1.47: `begin_reconciliation_before_styling` says no | KEPT for 1.46; for 1.47 it is the context compare alone (a size change that leaves the context equal passes) |
| any node declares `animation` | 1.46, 1.47 (`styled_dom_declares_an_animation`, O(n) lookups) | KEPT: transitions and moves are measured from the old cascade under the old interaction state; narrowing = capture per node in `style_reconciled_dom` (the old cascade is at hand there: the transition code of `begin_reconciliation` 18965-19075 with the node ids remapped); not built |
| a departing node declares `-azul-animation-out` | 1.47 | KEPT: the old result is moved into a zombie, whole |
| generated content (`::before`, list markers, footnotes) | 1.46: full path when the sheet makes any; 1.47: no gate - `cascade_reusing` keeps no row when a generated node exists and the tree is cascaded whole | NARROWED on 1.47 (reconciled first all the same) |
| a changed node is an image, icon, view, has `Updated`, other handlers' events, a dataset that came or went, a hint | 1.46 (`changed_nodes_are_adoptable`) | NARROWED: such a rebuild now takes 1.47, where the node is reconciled as on the full path (its events fire, its image is carried) and only IT is cascaded |
| a state came with the rebuild | 1.46: whole cascade; 1.47: the node whose state differs from its old node's is cascaded with what is below it | NARROWED on 1.47 |
| a drag state is raised | 1.45 / 1.46 / exit A: whole cascade; 1.47: no row kept | KEPT until A3-window-52 (`23ba0306f`) is merged: see "From the rows agent" 2 |
| the sheet has `var()` or custom properties | `recascade_nodes` says no: whole cascade | KEPT: the variable pass resolves DOM-wide (`custom_property_cascade`, not read) |
| more than half of the nodes are reached | `recascade_nodes`, `cascade_reusing` | KEPT: beyond half the whole cascade is cheaper than the ancestor-chain rebuild plus the splice; a label that only moved no longer counts (diff-first-11) |
| a sheet with `+` or `~` | 1.46: a changed node reaches its parent's whole subtree; 1.47: only the siblings AFTER a changed node (condition 4 is a chain) | NARROWED on 1.47; 1.46 as before (`nodes_reached_by_changes`: the later siblings only is the same change there, not made) |
| a sheet with `:first-child` / `:nth-child` / `:last-child` | 1.47: a node whose `CascadeInfo` moved is cascaded | new on 1.47, per node |
| an inactive window | 1.47: no row kept | KEPT: `:backdrop` is raised from the rows after the cascade |
| a runtime override on a node | 1.47: that node (and what is below it) is cascaded; then the override is migrated and the compact cache built whole (`migrate_user_overrides_from`) | per node; the whole compact build is the full path's too |

## api.json

No change. Everything added is Rust-internal (`azul_core::diff`, `azul_core::styled_dom`, `azul_core::prop_cache`,
`azul_layout::window`). Proposed and NOT made: `DomBuilder` / a layout callback that may return `FastDom`
(diff-first-09).

## Probe spans (`AZ_PROFILE=cpu`)

A rebuild of the window's DOM shows which exit it took: `regen_precascade_exit` (A), `regen_texts_only` (1.45),
`regen_nodes_changed` (1.46; inside it `regen_restyle_changed_nodes`, `regen_css_diff_changed_nodes`), or
`regen_style_user_dom` (the flatten on 1.47, the flatten and the cascade on the full path). On 1.47:
`regen_begin_reconciliation` holds `reconcile_dom`, `reconcile_transfer_states`, `reconcile_remap_node_ids` and no
`reconcile_arena_copy`; `regen_runtime_states` holds `regen_style_reconciled` (the cascade that keeps rows, or the
whole one) and `reconcile_css_diff`. A view's re-render has no span of its own for the cascade (it is inside the
view's pass). azul-core has no `Probe`: the stages inside `recascade_nodes` are one span (the counter
`nodes_cascaded_on_this_thread` says how many nodes it ran for).

## What to look at on the real window

Default ON for all five steps: each exit falls back to the full path (or the whole cascade) by itself wherever a
gate fails, and `AZ_DIFF_FIRST=0` gives the reference to compare with. A mistake in `tree` / `view` would show as
a node with the style of ANOTHER node (rows handed to the wrong node) or with its style of before the rebuild.
- Toggle something that changes a class or an inline style with a `RefreshDom`: the changed node shows its new
  style, its children too, nodes beside it do not move unless the layout says so.
- Open and close a panel, add and remove rows of a list, switch a tab that replaces a subtree (`tree`): the new
  nodes are styled, the nodes around them keep their look; with the pointer resting on a button that has a hover
  face (and on a row whose hover colour its text inherits) while the rebuild happens, the hover face stays, on
  the node AND on its text.
- A focused text field in a page whose other nodes come and go: the caret stays, typing continues.
- The editor's map (`view`): pan and let it settle; zoom; drag a label; select a label. Labels keep their fonts,
  colours and halos, labels that enter look like the ones of their class, no label is drawn at the place it had
  before the pan. Compare a settle with `AZ_DIFF_FIRST=text,nodes,style,tree` (views in full).
- A list whose rows exchange their texts on a rebuild (a sort without keys) while one row is hovered or focused:
  on exits 1.45 / 1.46 the state stays at the place, on the full path and on 1.47 it follows the content
  (`reconcile_dom`). Key the rows (`with_key`) where it matters.
- `AZ_PROFILE=cpu`: a panel toggle shows `regen_style_reconciled` and no `reconcile_arena_copy`.

## What the first design got wrong (found by reading the styling code in full)

1. "`adopt_rows` copies the row of every per-node table": the tables are not rows. `css_props` / `cascaded_props`
   are flat stores with shared runs, `computed_values` is transposed (a node list per distinct value), and the
   kept `cascaded_props` rows are PRUNED - a dirty child cannot inherit from them. Hence the ancestor-chain rebuild.
2. "the tag id KEPT": tags are `index + 1`, nothing to keep.
3. "an unchanged NODE" as `structure` + `style` equal: `structure` holds the child count. The second half does
   not use fingerprints for the node at all: it compares the node's fields on the two arenas.
4. (b) asked the sibling condition of every ancestor: only where a rule can read it (`+`, `~`, a structural
   pseudo-class); and `:nth-last-child`, `:only-child`, `:empty` do not exist in this engine.
5. Step 3's `pair_builds` "replaces `reconcile_dom`": it cannot, and it need not. Any pairing serves the styling
   when each pair is checked; `reconcile_dom` runs early instead.
6. The computed values of a node depend on the STATE of its parent (state overlays). See diff-first-10.
7. (second half) "an inline style is a field of the node": `Dom::with_css` hangs a SHEET on the node (scoped to
   it); only `NodeData::with_css` / `with_css_props` write the node's own style. The arena does not say which
   rules came with which node: the fingerprints' `sheets` tier does.
8. (second half) "unchanged subtrees keep their boxes" is not the shell's to give: the solver builds every
   RENUMBERED node fresh (06b).

## From the rows agent (A3-window, `## For area diff-first`): the four exact changes

1. Row 18's shell line (`|| styled_dom.needs_recompute_after_append()`): fixed 8cfbd5812. Its count is in
   `rows_that_come_are_styled_and_laid_out_and_no_other_node` (the full path: one whole cascade).
2. Row 52, the two `drag_state_raised` clauses of `regenerate_layout`: open: `23ba0306f` (A3-window-52) is not in
   `audit/integration` yet; the clauses become `if states_changed` once it is merged here (and the drag-state
   refusal in `cascade_reusing` goes with them).
3. Row 42, `merge_fresh_datasets` with `RefAnyHolders`: open: not started.
4. Row 15's remainder, `restyle_user_properties` with `recascade_nodes`: open: not started. Design: the write of
   an override to node x is a cascade of x and what is below it when the property inherits, of x's compact
   entries alone when it does not (`recascade_nodes(.., dirty = subtree of x or nothing, moved_in_place = {x})`
   reads `user_overridden_properties` in the compact builder's step 4b and in `apply_cascade_properties` step 5).

## Hand-over (kept current at every commit)

**Built (branch `audit/diff-first`, all `[unverified]`):**
| what | commit | tests to run |
|---|---|---|
| first half: steps `text`, `nodes`, `style` | 3d51fc7fa, 03af4217d, 5acd151cf, 009e311d8, 00b4d8305 | core `diff_test::audit_diff_first_tests`, `styled_dom_test::audit_diff_first_tests`; dll `headless::tests::a_rebuild_*`, `one_changed_*`, `an_identical_rebuild_*` |
| diff-first-10 (full path, inherited hover value) | f589b3743 | dll `a_full_rebuild_keeps_the_hover_colour_the_hovered_row_hands_its_text` |
| rows agent 1 (step 3.4's line) | 8cfbd5812 | dll `rows_that_come_are_styled_and_laid_out_and_no_other_node` |
| step `tree`, core (`unstyled_*`, `cascade_at_rest`, `spliced`, `cascade_reusing`) | 06480341b | core `styled_dom_test::audit_diff_first_tests::a_cascade_that_keeps_rows_*`, `sections_that_come_*`, `a_node_written_*`, `a_node_whose_own_sheet_*`, `an_unstyled_dom_*` |
| step `tree`, window and shell (exit 1.47, the solver's flags) | e332a6b34 | dll `a_rebuild_that_changed_the_tree_*`, `rows_that_come_*`, `five_hundred_nodes_*`, `the_steps_of_the_switch_*`, `a_changed_text_takes_the_full_path_*` |
| step `view` (+ the class "moved in place") | 43bb97f38 | dll `a_view_rendered_again_*`, `a_view_whose_labels_all_moved_*`, `a_view_that_exchanged_5_percent_*` |

**Most likely to fail compilation or to draw wrongly, first:** e332a6b34 (the shell's control flow around the
unstyled tree; `begin_reconciliation_before_styling`'s borrow of the old result) > 43bb97f38
(`reconcile_rematerialized_child` re-borrows the new arena after the cascade; the test closures of `dfv_mutate`)
> 06480341b (`cascade_at_rest` on `self`: the split of `create_from_compact_dom`; `spliced`'s struct update) >
f589b3743 > 8cfbd5812. If the split of the creation is wrong, EVERY window is affected (it is on the full path
too): `an_unstyled_dom_cascaded_at_rest_is_the_dom_the_creation_makes` and the whole existing core suite guard it.

**In flight:** nothing uncommitted beyond these notes.

**Left, in the order of the coordinator's priorities, each with its design in the block named:**
1. Views, the kept-DOM exits (`text` / `nodes` / `style` for a re-render that keeps the tree): diff-first-11,
   "NOT built". Saves the flatten and `reconcile_dom` of a settle (est. 17-30 ms at 25k nodes).
2. The solver across renumbering: diff-first-06b (A3-solver's and A8's files). It is what a pan that brings
   labels in the middle of the list still pays in layout.
3. The display list across a changed tree: diff-first-07.
4. Gates: the table above; narrowing `animation` to the nodes that declare one is the one with a design and no
   code.
5. The rows agent's changes 2-4 (above).
6. diff-first-05 (the hashes and maps of `reconcile_dom`), diff-first-08 (tags, the cache clone of exit 1.46,
   the animation lookup), diff-first-12.

## Not read

- `layout/src/solver3/*`: read `mod.rs` 840-1000 (the reconcile's entry, the skips) and `cache.rs` 1985-2440,
  3290-3700 (`reconcile_and_invalidate_restyled`, `DomPairing`, `DomDiff`, `reconcile_node`); nothing else.
  `display_list.rs`: `PatchState`'s fields and gate only.
- `core/src/prop_cache.rs` 3600-6050 only at the functions named in this file; `restyle_cascade` 2237-2945 at
  the fields it writes and its rule classification (2360-2570), not its inheritance walk (the copy of it in
  `recascade_nodes` was read in full); the `calc_*` and scrollbar getters not at all.
- `css/src/compact_cache.rs`: the struct, `with_capacity` and the px-overflow accessors; not the slot codecs.
  `core/src/compact.rs`: `build_compact_rows` 583-660, 846-1110, 1195-1396 (what a row is built from).
- `core/src/custom_property_cascade.rs`, `generated_content.rs`, `ua_css.rs`: the entry predicates only.
- `remap_node_ids`, `finish_reconciliation` (its head only), `layout_dom_recursive_with_viewport`,
  `follow_host_in_kept_view` (its head), the view manager.
- `text3/**`: nothing (06b needs it).
