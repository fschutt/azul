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
  labels after it (06b). A pan that exchanges nothing, a label selected or restyled: the document is kept
  (11b): no flatten, no pairing; 0 nodes cascaded and 0 boxes built for the pan
  (`a_view_whose_labels_all_moved_styles_no_node_and_keeps_every_box`), 2 nodes for the label
  (`one_changed_label_of_a_view_styles_that_label`); what is left is O(n) with small constants (the fingerprint
  of the callback's DOM, the clone of the cascade for the CSS diff, the compact clone, the tags: 08).

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
- the inventory (third successor, by grep and the reconcile's clone arm; text3 itself still not read in full):
  where a carried box holds DOM node ids that a renumbering makes stale -
  1. `LayoutNodeHot::dom_node_id` (set by `adopt_node_from_old(.., Some(new_dom_id))` already);
  2. the inline layout of an IFC root, `LayoutNodeWarm::inline_layout_result` and
     `inline_measure_layouts`: `source_node_id: Option<NodeId>` on the shaped items
     (`text3/glyphs.rs` 54, 193: the cluster and the glyph run; `text3/cache/inline_items.rs` 583;
     `text3/cache/shaping.rs` 601; copied through `logical_items.rs` 283), read by the solver in 13 places
     and by the list as a `Text` item's `source_node_index` and per-item `node_mapping`;
  3. `LayoutNodeWarm::inline_content_cache` (the collected inline content of a root: its runs name their
     source nodes; it re-validates itself by the subtree fingerprint, not by ids);
  4. the per-node cache entry (`LayoutCacheMap::remapped` moves it along `carried_indices`; its
     `abs_interior` / `layout_entry.child_positions` are keyed by LAYOUT index, which the reconcile re-points);
  5. what the WINDOW keeps by DOM id is remapped by `remap_node_ids` already (the managers), and a hit-test
     tag is `node index + 1`: the list items of a renumbered box are another matter - they are painted anew
     as long as the box is fresh, and once it is carried the list's across-trees patch must refuse its run
     (the tag and `node_mapping` inside it are the old id's): `build_across_trees` already re-emits a carried
     box whose DOM id is not its old box's, so 06b can land without touching the list.
- the steps, in order: (a) a function `renumber(&mut UnifiedLayout, &DomPairing)` in text3 that rewrites
  every `source_node_id` (and refuses - `None` - when an id has no new one), with a test that a layout
  renumbered by a shift equals the layout of the shifted DOM item for item; (b) in `reconcile_node`, where
  `renumbered` forces `DirtyFlag::Layout` today: carry the node (`adopt_node_from_old`) and run (a) over its
  inline layout, measure layouts and content cache, falling back to fresh when (a) refuses; a renumbered
  TEXT node carries too (its `ifc_membership` is re-pointed through `carried_indices` as for any clone);
  (c) `nodes_clean_for_layout` flags a renumbered node whose data reads alike (the flag is withheld today
  because it gains nothing); (d) a step name of its own (`renumber`, bit 256) around (b) and (c), and the
  equality walks `a_rebuild_that_changed_the_tree_*` / `a_view_rendered_again_*` as its guard, plus a count:
  a row inserted at the top of 1,200 builds the boxes of that row and no other.
- why not built in this session: (a) is a change inside text3's item types (A8's files) that has to be made
  by someone who has read every variant of the shaped items and every place a layout is shared (`Arc`) between
  the tree, the text cache and a window's result; done blind and partially, a stale id paints the wrong node's
  selection or hits the wrong node, silently.
- severity: high (it is what is left of "unchanged subtrees keep their boxes" when a node comes early)
- status: blocked (a): the cached inline layouts name DOM nodes by id (text3, A8's and A3-solver's files); the design and the inventory are this block; built short of it: the flags, every node that keeps its id keeps its box, and the list is patched across a changed tree (diff-first-07)

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
- as built (`dd0ef31d4`, step `tlist`): the map existed (`ReconciliationResult::carried_indices`, what the
  cache map is remapped with); `PatchState::build_across_trees` keys runs, deltas and sizes through it; the
  gate's fourth arm takes it when a producer named its cascaded nodes (`CascadedSince`: a changed tree is
  always a cascade that ran); exit 1.47 hands the names. The list is recorded as a FULL emission (the patch
  damage and the move summary pair boxes by one index; the renderers' item diff covers a changed tree as
  before). Read for it: `PatchState`, `build`, `try_copy_cached_run` and the copy loop, the gate and its
  bookkeeping, `reconcile_node`'s clone arm, `try_reuse_anon_wrapper`, `generate_tag_ids` (a tag is
  `node index + 1`). Why the ids inside a copied run stand: a carried box has the DOM id it had (a
  renumbered node is fresh), a box carried under another DOM node (paired by position) is re-emitted, and
  the inline root of a fresh member is re-emitted.
- tests: dll `rows_that_come_are_painted_and_the_rows_before_are_copied`,
  `a_box_that_got_another_layout_index_is_copied_from_the_run_it_had`,
  `labels_that_enter_a_view_are_painted_and_the_others_copied`; the walks with every step.
- what is left of it is 06b: every node after the first that came or went is renumbered, built fresh and
  painted.
- severity: high
- status: fixed dd0ef31d4

### diff-first-20 a rebuild that kept the window's DOM emitted the window's whole list
- file:line: `dll/src/desktop/shell2/common/layout.rs` exit 1.46 (`restyle_rebuilt_nodes`), exit 1.47
  (`style_reconciled_dom`); `layout/src/solver3/mod.rs` the patch gate
- class: wasted-work (the predecessor's table: "nodes changed where they stand: exit 1.46: list emitted in
  full"): the cascade over the reached nodes moves the epoch, the gate takes a reconciled tree only with no
  css dirt and an unmoved epoch. Est. 1.0-2.0 ms per 1,000 nodes of the window's DOM per rebuild.
- fix: the hand-over of diff-first-18 part 1 from the shell (`554d8ccd2`, step `wlist`): epoch before, epoch
  after, the reached nodes, under the conditions of the clean flags and only where the cascade ran for the
  reached nodes alone (`restyle_rebuilt_nodes` answers that too). Exit 1.47 hands its names under `tlist`
  (`dd0ef31d4`). Every rebuild voids what an earlier one named and no pass took.
- what it does not save: a change that moves the boxes after it (a taller row) paints them (diff-first-15).
- tests: dll `one_changed_class_paints_its_row_and_copies_the_windows_list`,
  `a_box_the_windows_cascade_reached_is_painted_though_it_is_clean`; the walks with every step.
- severity: high
- status: fixed 554d8ccd2

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
- the kept exits (diff-first-11b, `restyle_kept_child`): a re-render that keeps the tree and the sheets and
  changes nodes where they stand (a label selected or moved, a pan that brings none in) keeps the view's
  DOCUMENT: no flatten, no `reconcile_dom`, no remap (est. 0.7-1.2 us per node saved: 17-30 ms at 25k nodes); the
  changed nodes are written onto it and styled where they stand. It says no where the pairing has work (a changed
  key, an image, an `Updated` handler: `changed_nodes_are_adoptable`), for a document with a node in a state (a
  document rendered anew starts at rest; a kept one would keep the hover: taken to the pairing path instead, so
  there is NO stated difference from the full path here), another context or host values, nested views, a
  retired result. Identity follows the place, as on the root's exits 1.45 / 1.46: for keyed nodes that is the
  pairing `reconcile_dom` makes (an unchanged key at an unchanged place); for keyless nodes whose contents
  exchange places it is the difference stated under "What unchanged means".
- test: dll `a_view_rendered_again_leaves_what_the_full_path_leaves`,
  `a_view_whose_labels_all_moved_styles_no_node_and_keeps_every_box`,
  `a_view_that_exchanged_5_percent_of_its_labels_styles_5_percent`,
  `one_changed_label_of_a_view_styles_that_label`.
- severity: high
- status: fixed eba4d13e4 (the pairing path: 43bb97f38)

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

### diff-first-13 the kept exits told the solver "unchanged" for a node a callback had written
- file:line: `layout.rs` exits 1.45, 1.46 and the GRANULAR DIFF block; `layout/src/window.rs` `DomGenerations`
  (`bump_for_pass`, `written_since_pass`), `dom_written_since_its_pass`, `note_dom_laid_out`,
  `invoke_virtual_view_callback_impl`; `layout/src/solver3/cache.rs` `reconcile_node` (`diff_clean_here`: where
  the flag is believed)
- class: wrong-output (a written style is never laid out), on the DEFAULT path since the step `text`; found by
  the verifier: `dragging_the_slider_leaves_no_thumb_behind` (the thumb follows one move and stops)
- evidence: `set_css_property` from a callback writes the node's own style on the kept arena
  (`apply_node_css_change`: `upsert_inline_css_property`) and, in a shell, lays nothing out; the rebuild the
  callback returned (`RefreshDom`) does. That rebuild compares the app's two builds: the thumb reads alike in
  both (the app does not store the value), the caption's text changed: exit 1.45, `dom_diff_clean[thumb] = true`.
  The solver then takes over the thumb's fingerprint of its last pass - from before the write - and finds no
  dirt. With a state change in the same rebuild the exits hand over no flags, which is why the press and the
  first move worked.
- fix: a DOM a callback wrote since its last layout pass (its generation moved without a pass) gets no flags
  from any exit that keeps or compares it; the solver fingerprints every node of it and finds the write.
- what it costs: such a rebuild hashes every node again (200-500 ns each), as the full path does.
- not covered by a test: that flags ARE still handed over for a DOM nobody wrote (a mutant that calls every DOM
  written passes every test and loses the skip): `last_fingerprint_skips` of the text exit is the count to
  assert; added with the next commit that touches the dll tests.
- test: dll `dragging_the_slider_leaves_no_thumb_behind`,
  `a_style_a_callback_wrote_is_laid_out_by_the_rebuild_it_asked_for`.
- severity: high
- status: fixed 6c43652f1

### diff-first-14 one animated node anywhere sent every changed rebuild down the full path
- file:line: `layout/src/window.rs` `rebuild_can_restyle_in_place`, `styled_dom_declares_an_animation_among`,
  `begin_reconciliation_before_styling` (`styled_dom_declares_a_move`), `style_reconciled_dom`
  (`capture_declared_transitions`); `layout.rs` the gate of exit 1.46
- class: wasted-work (the gate `animation` asked the WINDOW, not the nodes a rebuild restyles)
- evidence: exits 1.46 and 1.47 gave up when any node of the window declared an `animation` (a switch, a tab
  strip, a spinner somewhere). A full rebuild starts a transition only on a matched node that declares one AND
  whose properties moved, and slides only a node that declares `move`.
- fix: exit 1.46 asks the nodes the changes REACH (none declares an `animation`; no node declares `move`); exit
  1.47 says no only for a declared `move` and for an exit animation, and starts the transitions of the nodes that
  did not keep their rows itself (`begin_reconciliation`'s per-pair rule, the old tree governing, the new node
  read in the old node's interaction state). A view's re-render starts no transition down any path.
- test: dll `an_animated_node_elsewhere_does_not_send_a_rebuild_down_the_full_path`;
  `an_identical_rebuild_and_a_changed_text_style_no_node` (the fingerprint skips diff-first-13 lacked).
- severity: medium
- status: fixed 6e4391abc

### diff-first-15 a patched pass translated the runs of the boxes it moved (the last bit of an item)
- file:line: `layout/src/solver3/display_list.rs` `DisplayListGenerator::try_copy_cached_run`,
  `PatchState::build` (`deltas[i] = new_positions[i] - prev_positions[i]`), `translate_item`
- class: wrong-output (an ulp per translation, and it accumulates); older than this task; the verifier's five
  equality walks (`hit area 14x14 @ (2, 59.8)` against `(2, 59.800003)`, passing with `AZ_NO_DL_PATCH=1`)
- evidence: an item of a box lies at `position + k`; a paint makes `fl(new + k)`, the copied run held
  `fl(fl(old + k) + fl(new - old))`. A run translated n times without being painted anew drifts by up to n/2 ulp
  from its node's laid-out position. The damage diff and the kept surfaces assume that an unchanged item is bit
  for bit the item a fresh pass makes; two windows that reached one DOM by a patched and by a full pass held
  two lists. Not a function of the diff-first steps: they make the patched pass the common one.
- fix (the coordinator's decision, on this branch): a run is copied only where it lies. In `try_copy_cached_run`
  a node whose delta is not exactly zero is painted anew (the run is popped first, so the node's later runs keep
  their order). Movement that should be cheap is expressed by frames (the move in place, a view's offset), not
  by translating items. `AZ_DL_TRANSLATE_MOVED_RUNS=1` restores the translation, for measuring.
- what it costs, per 1,000 moved boxes that emit items (a label: a span, its text of ~10 glyphs, a hit area;
  estimates, not measured): painted anew = two paint calls per box, each resolving the box's decorations (6-10
  property lookups of 50-150 ns) and emitting the glyph run from the kept inline layout: 1.5-3 us per box,
  **1.5-3 ms per 1,000**; translated and copied = an item clone and a float add per rect and glyph: 0.15-0.3 us
  per box, **0.15-0.3 ms per 1,000**. The counter `moved_runs_painted_anew_on_this_thread()` counts the paint
  calls. Where the window's DOM pays it (the passes that are patched): a text edit that changes a line count
  above other boxes (exit 1.45, and the full path's structure-preserved reconcile); a resize that moves boxes
  (right-aligned, centred, wrapped); and, dearest, an overrides-only frame of a LAYOUT tween (a height that grows
  above a long list shifts every box below it on every frame: +1.3-2.7 ms per 1,000 such boxes per frame).
- **the frames of a layout tween are the exception (the coordinator's decision, "way 3", second commit).**
  `8447aa202` painted EVERY moved box anew, which turned the user's ribbon guards red by design
  (`layout/tests/a_growing_ribbon_lays_out_only_the_boxes_it_resizes.rs`
  `an_azdrive_frame_lays_out_only_what_the_ribbon_resizes`: "what only moved keeps its layout and its items";
  `a_height_tween_re_emits_its_box_not_what_it_holds`) and cost every frame of a layout tween 1.5-3 ms per
  1,000 boxes below it. Built on top of it:
  - a frame of a layout tween that another frame follows translates a moved box's run as before the audit and
    RECORDS the box with the list it builds (`DisplayList::translated_since_painted`, layout indices; a box found
    recorded and copied where it lay stays recorded). The set is a field of the list: it goes where the list
    goes (the solver's kept list, the result's) and dies with it; a list a build painted in full has none.
  - every other pass - any pass that is no tween frame, and the frame that ENDS the tween - paints a moved box
    anew (`8447aa202`) and every recorded box too, moved or not, and records nothing. A list at rest is bit for
    bit a painted one; a list during a tween of n frames is up to n roundings off (bounded by the tween).
  - how the pass knows "a frame follows": the tick (`tick_animations`) drops a transition in the tick that ends
    it and arms the overrides-only latch for that last frame like for any other; the pass cannot tell. So the
    window says it at the head of its layout funnel (`layout_and_generate_display_list_timed`, not the tick):
    `OverridesOnlyStamp::more_frames_follow` = a transition of a layout property (`scope != None`) that is no
    longer delaying is still in `css_transitions`. The solver reads it with the latch (`tween_frame_followed`)
    and hands `PatchState::build` `translate_moved` = that, on the retained tree, and not a resize. The stamp is
    compared with `OverridesOnlyStamp::describes` now (the three fields it compared; `==` would take the flag
    for a claim about the DOM).
  - no frame is owed after the tween for this: the last frame IS the pass that paints. If the tween is cut off
    (a rebuild takes the override away, a resize, a hover restyle in between), that pass is no tween frame and
    paints the recorded boxes. If no pass at all follows a cut-off tween (the window hidden mid-tween), the
    list keeps its record and the next pass of any kind paints them; the cached-list hit of `layout_document`
    ("nothing changed: serve the kept list") refuses a list with a record, so that pass cannot serve it as it
    lies.
  - what a tween frame costs now: what it cost before the audit, plus one set insert per translated run.
  - the two guards, by reading (not edited): their measured frames are frames 2-4 of a 200 ms tween at 16 ms
    steps; after each tick the band's transition is still in `css_transitions` (t < 1), their `relayout` goes
    through the funnel, the pass is the overrides-only skip (the guards assert `last_reconcile_was_skipped` /
    `last_dl_build_patched` themselves) and no resize: `translate_moved` holds and `try_copy_cached_run` takes
    for every box outside the re-emit set the branch it took before `8447aa202` - the copy, with its
    `dl_patch_copy` span. `items_painted_again` and `re_emitted` are what they were, and so is what their
    `verify_patched_display_list` sees. The ribbon guard's tail ("run the tween out") compares layout rects.
  - a pass that is no tween frame DURING a tween (a GPU-only frame that would have been served the kept list, a
    hover restyle) paints the recorded boxes once and the next tween frame records them again: the price of
    "any pass that is no tween frame", paid only when such passes interleave with a layout tween.
- a view's moved labels after the step `view` (by reading, not measured): they do NOT reach
  `try_copy_cached_run` with a non-zero delta.
  1. A re-render that changed anything moves the document's cascade epoch on every exit (`recascade_nodes` ends
     with the bump, also when labels only moved; `spliced` on the pairing path; a whole cascade otherwise), and
     the patch's structure-preserved arm asks `last_cascade_epoch ==` the DOM's (`solver3/mod.rs`,
     `cascade_ctx_unchanged`): the view's list is emitted in full. That is diff-first-07's cost (the list of a
     re-rendered view is built whole), not this fix's.
  2. Where a view's pass IS patched (the resize-skip arm: the view's box changed size, its document did not,
     `window.rs` "Only the box moved"; an overrides-only frame): a label placed by `left` / `top` does not move,
     and a box whose own insets moved is `paint_dirty`, which is in the re-emit set asked BEFORE the delta. Only a
     box the new size moves (anchored right or bottom, centred, a percentage) is painted anew.
- the CPU blit stays valid. `compute_patch_move_summary` reads positions, sizes, the re-emit set, parents and
  opaque backgrounds - no item - and is computed for a pass that painted its whole list too
  (`patch.is_some() || same_nodes`): it never stood on copied items. The blit is taken
  (`cpurender/compositor.rs` `translate_hint_for_patch`) when the summary is of the sequence presented last, the
  dominant delta is a whole number of physical pixels (within 0.001) and at least one, the previous frame can be
  reused, the list was not shifted already and no scroll offset is non-zero. Inside it an item pair is a MOVE
  (blitted, no damage) when its old bounds lie in the moved region and `translate_item(old, dominant)` is the new
  item to the bit (`items_equal_translated`); every other pair is damaged, old and new bounds, and repainted over
  the blit. An item painted anew that is an ulp off the translated old one is therefore repainted, not blitted:
  right pixels, one small rect more. With a whole-pixel delta the two agree exactly unless the item's coordinate
  crosses a power of two toward the origin (128, 256, 512 px) with a fractional offset inside its box: about half
  of THOSE items repaint (est. under 5 % of a frame's movers). Before, a patched pass's copies agreed by
  construction; a pass that painted its list in full already lived with this.
- with A3-window-55 (`621a98aaa`, in `audit/integration`) merged, `try_copy_cached_run` reads: the `let delta`
  statement, then this fix's `if (delta.x != 0.0 || delta.y != 0.0) && !dl_translates_moved_runs() { count;
  return false; }` and the copied-runs count, then `let prev = patch.prev;` and the loop as 621a98aaa wrote it
  (`let binding = prev.color_binding(i).filter(|key| builder.bound_color_ids.contains(&key.id));`, its
  `continue` for a see-through rect that lost its key, `builder.next_color_binding = binding;`) ending in the
  unchanged `let item = translate_item(prev.items[i].clone(), delta);`. The two hunks do not touch.
- test: layout `audit_tests::diff_first::a_box_a_patched_pass_moved_is_painted_as_a_build_paints_it`,
  `the_switch_translates_a_moved_run_as_before`, `a_box_that_stayed_is_still_copied` (a resize: no tween frame),
  `a_frame_of_a_tween_translates_what_the_tween_pushes_within_a_rounding_per_frame` (forty rows climbing from
  3000 px to 40 px: per frame none painted anew, forty translated and recorded, each rect within a float's step
  per frame of the painted one, and some rect really off by the middle),
  `the_frame_that_ends_a_tween_and_a_pass_that_is_none_leave_a_painted_list` (after the last frame, and after
  a resize in the middle: the list is the painted list bit for bit and records nothing),
  `a_tween_cut_off_by_a_rebuild_leaves_a_painted_list`,
  `the_other_extreme_paints_what_a_tween_pushes_on_every_frame`; the five dll walks
  (`a_rebuild_down_the_diff_first_path_leaves_what_the_full_path_leaves` and its siblings) as they are; layout
  `audit_tests::a3_window::a_box_that_moved_in_a_pass_is_painted_as_a_fresh_layout_paints_it` (the window
  agent's, which stands on this fix).
- severity: medium
- status: fixed 8447aa202 (a moved box is painted anew) + the commit after diff-first-17 (the frames of a tween translate and record; hash in the hand-over table)

### diff-first-16 positions are floats, so no translation of an item is exact
- file:line: `layout/src/solver3/mod.rs` `PositionVec`, `LayoutNodeHot::used_size`, every rect of
  `DisplayListItem`; the cause behind diff-first-15
- class: design; what it costs is diff-first-15's "painted anew" where a copy would do
- design, in three lines:
  1. the solver hands out positions and sizes in fixed layout units (an `i32` of 1/64 px), rounded once where
     a box is placed; `calculated_positions`, `used_size` and the rects of a display list hold units;
  2. a paint adds `position + k` in units (`k` rounded once, where the cascade resolves it): every sum and
     every translation is exact and associative, `old + (new - old) == new`, a run copied n times is the run
     painted;
  3. `try_copy_cached_run` then copies moved runs again (the switch's behaviour becomes the default),
     `items_equal_translated` is exact for every delta, and a float appears only at the rasteriser's edge
     (`units / 64 x dpi`).
- severity: medium
- status: blocked (a): the number type of the solver's output, of every display-list rect and of the rects callbacks read (`LogicalRect`): A3-solver's, A3-core's, both renderers' files, api.json and every golden

### diff-first-19 a state that hands a value down cost every other node its inherited UA values
- file:line: `core/src/styled_dom.rs` `recompute_inheritance_for_states` (f00b2a9f9, older than the audit), its
  two callers `restyle_states` (every hover / focus / active change that moves an inheritable property) and
  `let_raised_states_in` (diff-first-10, f589b3743: every full rebuild under such a state);
  `core/src/prop_cache.rs` `compute_inherited_values`, `prune_compact_normal_props`
- class: wrong-output (a value missing from `computed_values`); a real defect, and NOT the one the report read
  into it. Found by the verifier's three "under the pointer" walks once the f32 bit no longer hid it: `style 1`
  holds `TextColor(#000000ff)` in the diff-first window and not in the full-path window.
- which window is right: the DIFF-FIRST one. Nothing writes an explicit black: `#000000` on the list (node 1) is
  the root's UA text colour, INHERITED, read from `computed_values`; a DOM cascaded from scratch has it on every
  node that declares no colour (the walks without a pointer show it in both windows). The full-path window
  LOST it. Line 102 is the first difference because node 0, the root, answers the UA default itself.
- cause: `recompute_inheritance_for_states` ran the inheritance pass alone over the rows as they lay. A cascade
  prunes `cascaded_props` once the compact cache is built (the resting values with a compact encoding: the UA's
  defaults among them), and the pass computes a node's inherited values FROM those rows. Run after the prune,
  it rebuilt `computed_values` without every value whose declaration was a pruned row: the root's text colour
  for the whole DOM, a link's UA colour for the link's text (the link then reads its parent's colour). The
  compact cache, rebuilt beside it with the UA table, kept the right values.
- seed 1, step 0 by hand (kind 2, steps [Text, Nodes]): the pointer moves over a row (`.df-row:hover { color }`
  is inheritable, so both windows ran the pass and both lost the colour); row 1's class changes; the kept
  window runs the whole cascade in place (no step `style`) and has every value again; the full-path window
  cascades at rest (complete), prunes, lets the hover in and runs the pass: lost again. Steps of kind "text"
  (exit 1.45, no cascade) left both windows equally wrong, which is why the walks with more steps fail later.
- what it does in production: after any hover over a node whose `:hover` sets `color` (or any inheritable
  property), and after any full rebuild under such a pointer, `get_property` answers no value (or an
  ancestor's) for an inherited compact-encoded property on every node that takes it from the UA: under a dark
  theme the root's light text colour is what goes missing. Text is painted from the compact cache and keeps its
  colour; a reader that asks `get_property` for an inherited colour falls back to its own default. Which
  readers do was not enumerated.
- fix: the subtrees of the nodes that hand a value down are styled again by the cascade over named nodes
  (`StyledDom::recascade_subtrees` -> `restyle_changed_nodes`, which rebuilds the ancestors' pruned rows
  first), under the states the nodes are in; nothing else is touched. Both callers use it: the state-change
  path names the nodes whose inheritable property moved (entered and left), `let_raised_states_in` the nodes in
  a state that hand a value down. Where that cascade says no (a sheet with `var()`, more than half of the
  DOM) the whole cascade runs, pruned. `recompute_inheritance_for_states` itself is that whole cascade now.
- cost: a hover that hands a colour down cascaded nothing and rebuilt the inherited values and the compact
  cache of EVERY node (est. n x 0.8-1.5 us: 3-5 ms at 3,500 nodes); now it cascades the two subtrees (est.
  2-6 us per node below the hovered and the left node: 20-60 us for a row of ten nodes). With `var()` in the
  sheet it is the whole cascade (n x 2-6 us): dearer than before, and right.
- test: core `styled_dom_test::audit_diff_first_tests::a_state_that_hands_a_colour_down_costs_no_other_node_what_it_inherits`
  (the UA's link colour, which is not black: before the fix the link and its text answer the list's colour);
  dll the three walks `a_rebuild_that_changed_nodes_under_the_pointer_*`,
  `a_rebuild_that_changed_the_tree_under_the_pointer_*`, `a_rebuild_that_styled_its_changed_nodes_under_the_pointer_*`.
- second defect, in the fix itself (found by the verifier once the test had a true premise: an `<a>` needs an
  `href` to be `:link`): the cascade over named nodes runs its inheritance pass over rows BEFORE their prune. A
  child's inherited row holds a COPY of what its parent holds AT REST, and `apply_cascade_properties` step 2
  applied the copy over the hover value the child had just inherited (`should_apply_cascaded` lets a row entry
  beat an `Inherited` one). `get_property` answered the resting colour for every node below a hovered one
  (#222222 for #00aa00), the compact entries the hover colour: the two stores disagreed. The whole pass over
  unpruned rows (`restyle` under a state) did the same; only a pass over PRUNED rows came out right, which is
  why the old code did - and lost the UA rows with the copies.
- fix of the second defect (`3afb0f554`): both inheritance passes keep, per node, the inheritable types it shows
  its children from a state (its own, or handed on from above while it holds the type as an inherited value);
  a non-UA resting copy of such a type is passed over, a UA default applies as ever; for the KEPT parent of a
  named node the set is read from the cache (`types_shown_from_a_state`, a walk up, once per kept parent).
  `get_declared_property_ranked` tier 3 answers the node's inherited value where such a copy is still in the
  row (a property whose copy survives the prune: `cursor`, `text-decoration`, `user-select`; rows that were
  not pruned). At rest the sets are empty: no value and no origin changes.
- tests of the second defect: the block's test, extended (both stores for every node below the box; nodes
  styled again below a kept parent; a heading with its own colour; a link; the state lowered: every node of the
  page back at rest in both stores), and `a_state_hands_down_a_property_whose_copy_survives_the_prune`. The
  mutant of each assertion is in the test's doc comment and in the commit message.
- severity: medium
- status: fixed 3afb0f554

### diff-first-18 a view that renders again has its whole list emitted, although the step `view` kept most of it
- file:line: `layout/src/solver3/mod.rs` the patch gate (`cascade_ctx_unchanged`: `cache.last_cascade_epoch ==`
  the DOM's; `structure_ok`), `css_dirty_reemit_set`; `core/src/prop_cache.rs` `recascade_nodes` and `spliced`
  (both end with the epoch moved)
- class: wasted-work (the coordinator's question after diff-first-15; answer by reading, no code)
- why a moved epoch forbids the copy today: a run is a function of its box's layout (position, size, inline
  layout) and of the PAINT values the cascade resolved for its node: backgrounds, borders, radii, shadows,
  colours, decorations, visibility, the keys of transform / opacity / state colours. The reconcile sees node
  data and states, not the cascade's result (a theme flip restyles a node whose data did not change). Nothing
  IN a run depends on the epoch: the epoch is the solver's only proof that no cascade ran since the list was
  built, a stand-in for "no node's paint values moved". When it moved, the solver does not know WHICH nodes
  were restyled, so it copies none.
- what the step `view` knows and does not hand over: exactly which nodes were cascaded again (the complement of
  `unchanged` from `cascade_reusing` / the `reached` set of `restyle_kept_child`; a node below a cascaded one is
  cascaded too, so inherited paint is covered), which kept their rows (their computed values are the same rows:
  identical by construction) and which only moved in place (their box is `paint_dirty` already).
- what it would take, in two parts:
  1. the tree kept (labels restyled, selected, moved; no label came or went): hand the solver the cascaded
     nodes with the pass (a one-shot latch beside `dom_diff_clean`, "the epoch moved through a step that named
     its nodes"), accept a moved epoch in the structure-preserved arm when that latch is set, and put the
     named nodes' boxes into the re-emit set (`css_dirty_reemit_set`'s rule, which the retained-tree arm
     applies already: the box, and what it holds unless the change is box geometry only). Est. 40-60 lines in
     `layout_document` and the two shells of the step; the equality walks of the view
     (`a_view_rendered_again_leaves_what_the_full_path_leaves`) are its guard, plus a count test (one changed
     label of 1,000: one box painted).
  2. the tree changed (labels came or went): the runs are keyed by the OLD layout index; `PatchState` needs
     the old index of each new box (diff-first-07) and the solver must keep the boxes of renumbered nodes
     (diff-first-06b), or every renumbered node is fresh and re-emitted anyway.
  A label that MOVED is painted anew in any case (diff-first-15: no tween frame). So the copy pays for the
  labels that stood: all of them when a pan is expressed by the view's offset, none when every label gets a
  new `left` / `top`.
- estimate (not measured), the editor's view of about 12,500 nodes and 17,500 items: emitting today 1.0-2.0 ms
  per 1,000 nodes (a label's box: two paint calls with 6-10 property lookups and its glyph run, 1.5-3 us; a
  text node without a box of its own: about 0.2 us), so 12-25 ms per settle; copying 0.1-0.25 ms per 1,000
  nodes (an item clone per run), so 1.5-3 ms, plus what stays per node either way (the stacking-context
  walk and the tag lookups, est. 0.1-0.2 ms per 1,000). A settle that kept every label would go from 12-25 ms
  to 3-5 ms of list emission.
- severity: high for the editor's settle (after styling and layout it is the largest part left)
- part 1, the exact plan (decided by the coordinator for the second round; NOT started: this agent's context
  ran out at the clean commit `8e9ead705`):
  1. `layout/src/solver3/cache.rs`: a one-shot latch on `LayoutCache` beside `dom_diff_clean`, e.g.
     `diff_names_every_restyle: bool` ("the pass's css dirt and its `dom_diff_clean` flags together name every
     node whose cascade answers anything else than before"). `LayoutCache` has exhaustive literals: three in
     `layout/src/window.rs`, one in `layout/src/cpurender/raster.rs`.
  2. `layout/src/window.rs`: set it where the step `view` hands the child's cache its flags (the callers of
     `reconcile_rematerialized_child` / `restyle_kept_child` in `invoke_virtual_view_callback_impl`, where
     `dom_diff_clean` of `child_layout_caches[child]` is written), only when the new step is on, the flags are
     handed (`!written_since_its_pass`) and the css diff was made by `css_diff_between_knowing` over every node
     that did not keep its rows (both kept exits and the pairing path do that).
  3. `layout/src/solver3/mod.rs`, `layout_document`: before `dom_diff_clean` is taken for the reconcile, when
     the latch is set, keep the DOM nodes with `clean == false`. At the patch gate: `structure_ok` also holds
     for `last_reconcile_structure_preserved && latch && context_unchanged` (the epoch is NOT asked: the latch
     says what it stood for); `css_dirty_reemit` is `css_dirty_reemit_set(&new_tree, css_dirty, false)` in
     that case too; and the boxes of the unclean nodes (`new_tree.dom_to_layout`) join `reemit`, and
     `reemit_full` for the move summary. Everything else of the gate stays (fresh, retargeted, paint-dirty,
     size-changed and reflowed boxes re-emit already; a moved box is painted anew by diff-first-15: a re-render
     is no tween frame).
  4. the switch: `DiffFirstStep::List` (`ALL: [Self; 6]`, bit 32, name `list`; the test
     `the_steps_of_the_switch_are_named_one_by_one` lists the names), asked in 2; with it off the latch is never
     set and a view's re-render emits in full (`AZ_DIFF_FIRST=text,nodes,style,tree,view` = today).
  5. counters: `display_list::cached_runs_copied_on_this_thread()` exists; a twin for paint calls a patched
     build made itself (count in `paint_node_background_and_border` / `paint_node_content` after a `false` from
     `try_copy_cached_run` while a patch is armed), so a test can say "5 % painted, the rest copied".
  6. tests (dll, with the `dfv_*` helpers of the step `view`): (a) a view rendered again unchanged: no pass at
     all today (identical fingerprints) - so the test is "one label's class changed": that label's runs
     painted, every other run copied, the list bit for bit the full emission's (`verify_patched_display_list`
     on the child, or the walk `a_view_rendered_again_leaves_what_the_full_path_leaves` with the step on);
     (b) 5 % of the labels exchanged: needs part 2 (the tree changed); with part 1 alone it is "5 % of the
     labels restyled where they stand"; (c) every label moved: every label painted (each is `paint_dirty` and
     moved), the count says so; (d) the mutant: the unclean boxes not added to `reemit`: the restyled label
     keeps its old colour in the list and the walk names its item.
  7. to read first and answer under "## For area A7-editor (m4p)" (not read by this agent): the editor's
     `render_map_view` in the m4p integration worktree, `mapedit3/src/ui/main/map.rs`: do the labels' own
     `left` / `top` change on every settle (then part 1 gains nothing for a pan settle: every label is moved
     and painted), or does the view's content offset move while the labels keep their insets (then every
     label that stays is copied)? What the editor must do for the gain: build label positions in a coordinate
     frame that does not move with the pan (world or tile-anchored coordinates as `left` / `top`), and express
     the pan as the view's offset / one transform on the labels' parent, so that a settle changes the insets
     of no label that stays.
- part 1 as built (`aa929c920`, step `list`); where it differs from the plan above:
  - not a bool latch but a statement with its proof: `solver3::cache::CascadedSince { from_epoch, to_epoch,
    nodes }` in `LayoutCache::cascaded_since_list`. The pass keeps it only when its cached list was built at
    `from_epoch` and the DOM stands at `to_epoch`: a hover restyle, a user override or a context offer between
    the list and the render, or between the render and the pass, voids it (the whole list, as before). A bool
    beside `dom_diff_clean` could go stale (a pass that never ran, another producer's flags).
  - `nodes` is the complement of the clean flags (cascaded or moved in place), from both exits of the step
    `view`; from the kept exit only when the cascade over named nodes ran (`restyle_kept_child` answers a
    fourth value), not after the whole cascade in place.
  - the re-emit set of the new arm: `css_dirty_reemit_set(tree, css_dirty, false)` plus the boxes of the named
    nodes, each with the root of the inline context it is a member of (`boxes_painted_for`: a text's glyphs
    are in its root's content run). It joins the damage set and the move summary's set with it.
  - the early exit of a pass whose reconcile is clean (Step 1.5: paint-only css dirt) emitted in full too: it
    patches now, with the cached tree and positions on both sides, and records a full emission as before.
  - counters: `display_list::lists_patched_after_a_named_cascade_on_this_thread()`,
    `runs_painted_by_patched_builds_on_this_thread()`, `cached_runs_copied_on_this_thread()`.
  - tests (dll): `one_changed_label_of_a_view_paints_that_label_and_copies_the_rest`,
    `a_view_that_restyled_5_percent_of_its_labels_paints_5_percent`,
    `a_view_whose_labels_all_moved_paints_every_label`,
    `a_node_the_cascade_reached_is_painted_though_its_box_is_clean` (the mutant), each against a window with
    every step but `list`, the view's document and list compared line for line; the walk
    `a_view_rendered_again_leaves_what_the_full_path_leaves` runs with `list` among its steps.
  - not settled by reading: whether anything between the hand-over and `layout_document` moves the epoch of
    the child's DOM in the common case (then no list would ever be patched: the tests' premise `patched == 1`
    says so); whether an existing test pins a view's re-render as a FULL emission (`last_build_was_patched`,
    a damage rect list).
- what part 1 gains in the editor: nothing on a pan settle and nothing on a zoom settle, as the editor
  places its labels today (see "For area A7-editor (m4p)": every label gets new insets at every settle). It
  gains on a re-render that keeps the tree and most insets: a selection, a label moved or cut by hand, a key's
  message, the hover (`refresh_view`): est. 12-25 ms of list emission down to 1.5-3 ms of copies.
- part 2 (the tree changed): the list is patched across the trees since `dd0ef31d4` (diff-first-07): the
  labels BEFORE the first one that came or went keep their node ids and are copied
  (`labels_that_enter_a_view_are_painted_and_the_others_copied`). The labels after it are renumbered, built
  fresh by the solver and painted: that is diff-first-06b, and nothing else of this row is open.
- status: fixed dd0ef31d4

### diff-first-17 the first half's core tests took `Dom::with_css` for a node's own style (seven red tests)
- file:line: `core/src/diff_test.rs` `audit_diff_first_tests` (`page`, the nested tree); `core/src/styled_dom_test.rs`
  `audit_diff_first_tests` (`page`, `rebuild_onto`, `walk`)
- class: a defect of the FIXTURES, not of the pipeline (the verifier's seven failures of azul-core on integration
  `a23a72fde`); the code is right in all seven and is not changed
- the four of `diff_test.rs`: the fixture gave a span its "inline style" with `Dom::with_css`, which hangs a
  scoped SHEET on the node (`Dom::set_css`: `add_component_css(Css::parse_scoped(..))`); the node's own style is
  `NodeData::set_css` / `with_css_props`. So `sheets` was not empty (`the_subtree_sizes_are_the_tree`), a changed
  "inline style" was a changed sheet and `same_tree_changes` answered `None` as documented
  (`a_rebuild_that_kept_the_tree_names_the_nodes_it_changed`, `the_changed_nodes_are_written_onto_the_kept_arena`),
  and the page with a component sheet listed two sheets (`a_rebuild_that_changed_the_tree_or_a_sheet_is_not_named`).
  Fixture corrected: the spans carry their OWN style (`span()`); each test still fails on its mutant (the `style`
  tier not compared: "the second span's style"; `subtree_len` not compared: the nested tree, which now holds no
  sheet, so only the tree differs; `sheets` not compared: the red and the blue sheet).
- should the code NAME a node whose scoped sheet changed? No, and it is no hole. Naming a node hands the rebuild
  to exit 1.46, which writes the node onto the kept arena and cascades it against the sheets the kept DOM was
  created with: it has no way to exchange a sheet (the sheets left the `Dom` at creation, `take_css_from_dom`).
  `None` from `same_tree_changes` does not mean "nothing changed", it means "not this exit": a changed `with_css`
  changes the node's `style` hash and the `sheets` list, so the fingerprints differ (no identical exit),
  `texts_alone_changed` wants equal `style` (no exit 1.45), `same_tree_changes` wants equal `sheets` (no exit
  1.46), and the rebuild goes on to exit 1.47 - where `cascade_reusing` compares each node's sheet hash of the
  two builds and cascades the node and everything below it again
  (`a_node_whose_own_sheet_changed_is_cascaded_again`) - or to the full path. Pinned now in
  `a_rebuild_that_changed_the_tree_or_a_sheet_is_not_named` (a `with_css` that changed: listed, unequal
  fingerprints, `None` from both diffs); the dll equality walks change a row's `with_css` on every other step
  (`DF_PLACES`) and compare with the full path.
- the three walks of `styled_dom_test.rs`, seed 1, step 0 by hand: kind 6, picks [38, 37, 33, 42, 47, 6], nine
  sections: section 2's kind becomes 42 % 4 = 2 (it was 2), section 6's text 6 % 4 = 2 (it was 2), section 1's
  place 47 % 5 = 2 (it was 0). So the step's only change is `PLACES[2]` on section 1's box, node 9 - through
  `Dom::with_css`, a sheet. `rebuild_onto` asked `same_tree_changes`, got `None` (a sheet came) and returned
  through its `?` WITHOUT touching the kept DOM; the walk took `None` for "alike" and compared the untouched
  DOM with a new one: node 9's matched rules, rows and `width` / `margin-left` differ (the 9 differences). The
  window does not do that (see above). Fixture corrected: the walks build `page_own` (the place is the box's
  own style, which IS a change where the node stands, and what the walk's doc says it changes), and the walk
  now stops with "harness" when a step changed the page and was not rebuilt. `page` keeps hanging sheets: the
  walks of the step `tree` stand on that.
- what this does NOT prove: with the corrected fixture the three walks style a changed OWN style through
  `recascade_nodes` for the first time in any run (the inline declarations of the node, inherited ones for what
  is below it, `inline_overruled`). If they are still red after this, that is a real defect of exit 1.46's
  cascade, and the message names the node and the property.
- a wrong detail in the message of 8420ec290 (branches are append-only): the mutant "`subtree_len` not compared"
  answers `Some([1, 4])` for the nested tree (the row's child count moved too), not `Some([4])`; the test fails
  on it either way (it wants `None`).
- severity: medium (seven red tests; no wrong output)
- status: fixed 8420ec290 (`diff_test.rs`) and the commit after it (`styled_dom_test.rs`)

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
  - `list`: a view that renders again under `view` hands the pass the nodes it cascaded
    (`solver3::cache::CascadedSince`) and the pass patches the view's display list where the tree kept its
    shape (diff-first-18 part 1). Off: the view's whole list is emitted whenever its cascade epoch moved.
    `AZ_DIFF_FIRST=text,nodes,style,tree,view` = the state before it.
  - `wlist`: the same for the window's own list on exit 1.46 (diff-first-20): the shell hands the pass the
    nodes the changes reach. Off: exit 1.46 emits the window's whole list.
  - `tlist`: the list across a changed tree (diff-first-07): the pass copies the runs of the boxes its
    reconcile carried over (`PatchState::build_across_trees`), for the window and for a view; and exit 1.47
    hands the pass the nodes it cascaded. Off: a pass whose tree changed its node count emits its whole
    list, and exit 1.47 names no node.
  Examples: `AZ_DIFF_FIRST=text,nodes,style` = the state before the second half; `AZ_DIFF_FIRST=tree` = every
  changed rebuild of the window's DOM down exit 1.47; `AZ_DIFF_FIRST=text,nodes,style,tree` = everything but views.
A test picks its steps per thread: `window::set_diff_first_steps_on_this_thread(Some(&[..]))`,
`set_diff_first_on_this_thread(Some(bool))`. Which path a rebuild took: `window::kept_dom_rebuilds_on_this_thread()`
(exit 1.46), `window::tree_rebuilds_that_kept_rows_on_this_thread()` (exit 1.47 and a view's re-render, when rows
were kept), `styled_dom::nodes_cascaded_on_this_thread()`, `styled_dom::compact_cache_rebuilds_on_this_thread()`
(whole cascades). The pre-cascade exit for an identical rebuild is older than the switch and not under it.
For the step `list`: `solver3::display_list::lists_patched_after_a_named_cascade_on_this_thread()` (the passes
that took its arm), `runs_painted_by_patched_builds_on_this_thread()` and `cached_runs_copied_on_this_thread()`
(what a patched build painted and what it copied), `lists_patched_across_trees_on_this_thread()` (the arm of
`tlist`). The switch's bits are a `u16` since `list` (`diff_first_steps_of` answers one); eight steps.

`AZ_DL_TRANSLATE_MOVED_RUNS` (read once per process, at the first patched build that copies a run;
`layout/src/solver3/display_list.rs` `moved_run_policy`, `MovedRunPolicy`; diff-first-15):
- unset, or any value but `1` and `0` (`TweenFramesTranslate`, the default): a patched display-list build paints
  a box that MOVED anew, except on a frame of a layout tween that another frame follows: that frame copies the
  box's run shifted by the move and records the box with the list; the tween's last frame and every pass that is
  no tween frame paint the recorded boxes anew. A list at rest is a painted one.
- `1` (`AlwaysTranslate`): the behaviour before the audit, for measuring: every pass copies a moved box's run
  shifted by the move, nothing is recorded, nothing is painted anew for it. The list of such a pass is not the
  list a build makes (a rounding per translation, without end).
- `0` (`NeverTranslate`): the other extreme, for measuring: a moved box is painted anew by every pass, the
  frames of a tween too (what `8447aa202` alone did).
A test picks it per thread: `display_list::set_moved_run_policy_on_this_thread(Some(MovedRunPolicy::..))`. What
the patched builds of a thread did, per paint call: `display_list::moved_runs_painted_anew_on_this_thread()` (a
box that moved, no tween frame), `moved_runs_translated_on_this_thread()` (a tween frame, or `1`),
`translated_runs_painted_anew_on_this_thread()` (a box that stood but had been translated: the paint that brings
a list to rest), `cached_runs_copied_on_this_thread()` (every run taken from the list before).
`AZ_NO_DL_PATCH=1` (older, not this task's) turns the patch off altogether.

## The gates (what sends a rebuild down the full path, or the whole cascade)

Status per gate: KEPT (why it must stay) or NARROWED (what it is now). "Full path" = style at rest, then
`begin_reconciliation`; "whole cascade" = every node styled, inside an exit that otherwise holds.

| gate | where | status |
|---|---|---|
| the window's theme, app theme or rice moved (`theme_rebuild`) | every exit | KEPT: another sheet or UA table over the same nodes; no row stands |
| the decoration mode changed, or decorations are injected | 1.45-1.47 | KEPT: the injected tree is appended to a STYLED tree (`csd.rs`), ids shift |
| window size or selector context moved with the rebuild | 1.46: full path; 1.47: `begin_reconciliation_before_styling` says no | KEPT for 1.46; for 1.47 it is the context compare alone (a size change that leaves the context equal passes) |
| a node declares `animation` | 1.46: only when a node the changes REACH declares one, or any node declares `move` (`rebuild_can_restyle_in_place`); 1.47: only a declared `move` (`styled_dom_declares_a_move`) | NARROWED (diff-first-14, 6e4391abc): exit 1.47 starts the transitions of the nodes that did not keep their rows itself (`capture_declared_transitions`); `move` is KEPT: its first rect is captured in `begin_reconciliation`'s order |
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
`nodes_cascaded_on_this_thread` says how many nodes it ran for). In a patched display-list build:
`dl_patch_copy` (a run copied where it lies), `dl_patch_empty` (a box that emitted nothing, again nothing) and,
new with diff-first-15, `dl_patch_moved` (a box that moved in a pass that is no tween frame: its run is not
copied, the paint call that follows is made anew) and `dl_patch_settled` (a box that stood, whose run a tween
frame had translated: painted anew). A tween frame's translated runs count as `dl_patch_copy`.

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

## For area A3-core (the display-list patch): the last bit of a translated item

Reported by the verifier on the first half (five equality walks, one item each differing in the last bit of an
f32: `hit area 14x14 @ (2, 59.8)` on the kept-DOM window, `(2, 59.800003)` on the full-path window; the walk passes
with `AZ_NO_DL_PATCH=1`). The `node` lines of the dumps are equal: the LAYOUT positions agree to the bit. The item
differs.

- where: `layout/src/solver3/display_list.rs` `PatchState::build` (6160: `deltas[i] = new_positions[i] -
  prev_positions[i]`), `DisplayListGenerator::try_copy_cached_run` (`translate_item(prev.items[i].clone(), delta)`)
  and `translate_item` (1100: `origin += delta`). The solver's patched pass (`dl_patched_pass`), older than this
  task; not `node_patch.rs` (the window's move in place) and no function of the diff-first steps.
- class: wrong-output (one ulp), sign/offset
- evidence: a copied run of a node that MOVED is given `old_rect + (new_pos - old_pos)`. A fresh paint computes
  the rect from `new_pos` (for a hit area or a background: the node's position itself). In f32,
  `a + (b - a) != b` in the last bit for most pairs (the subtraction rounds, then the addition rounds again):
  with `a = 45.8`, `b = 59.800003` the patch gives `59.8`. The two windows of a walk patch at different rebuilds
  (a kept DOM keeps the tree's shape more often than a new one whose CSS diff names a node), so their items carry
  different rounding histories; a list that was patched k times carries k roundings.
- why it shows on the diff-first exits: they do not produce the value; they make the patched pass the common
  one. On the full path alone the same happens after any text edit or resize that shifts boxes.
- fix (exact, 4 lines, in `try_copy_cached_run` after the delta is read; the run is already popped, so it stays
  consumed):
  ```
          let delta = patch.deltas.get(node_index).copied().unwrap_or_else(LogicalPosition::zero);
  +       // A node that moved is painted anew: `old + (new - old)` is not, to the bit, the rect a
  +       // paint from the new position gives, and the next fresh list would differ from this one
  +       // in every such item. A run is copied only where it lies.
  +       if delta.x != 0.0 || delta.y != 0.0 {
  +           return false;
  +       }
  ```
  `translate_item` is then called with a zero delta only (its early return). Cost: a pass that shifts boxes (a
  line more in a paragraph above them) paints the shifted boxes again instead of translating their runs; boxes
  that did not move are still copied. `compute_patch_move_summary` (the CPU blit by the dominant delta) reads
  positions, not items, and is not touched.
  An exact translation is not available from the item alone: the paint computes `new_pos + offset` with the
  item's offset inside its node, which the stored rect holds only rounded.
- test: the five walks without `AZ_NO_DL_PATCH`; a unit test of its own: a list whose middle paragraph grows by
  a line, patched, equals the list built fresh, item by item.
- severity: medium
- status: fixed on this branch by the coordinator's decision: diff-first-15 (the four lines, with the switch `AZ_DL_TRANSLATE_MOVED_RUNS=1`, two counters and three tests). The window agent made the same change as A3-window-56 (`25347d2ca`) and reverted it (`8eca913a0`): one copy, this one

## For area A7-editor (m4p)

Read: `mapedit3/src/ui/main/map.rs` of the m4p integration worktree, `render_map_view`, `to_screen`,
`to_picture`, `place_labels`, `place`, `boxed`, `tile_node`, `pan_view`, `ViewMotion`. Nothing was changed there.

**The question: do the labels that STAY in view keep their typed `left` / `top` across a settle? No, neither
across a pan settle nor across a zoom settle.**
- a label piece is placed by `place(&mut span, class, x, y - b, angle)` with `(x, y) = to_picture(project,
  p.position.x, p.position.y - baseline_mm)`, and `to_picture` is `to_screen` less `motion.shift`;
  `to_screen(x_mm, y_mm) = ((x_mm + ui.pan.x) * s, (y_mm + ui.pan.y) * s)`. `render_map_view` sets
  `motion.shift = (0, 0)` and `motion.offset = (0, 0)` at its start ("a fresh picture: no shift, no zoom"). So
  every render places every node in VIEW coordinates of the pan it renders at: after a pan by (dx, dy) px
  every label, symbol, block, grid number, tile and the paper get `left + dx`, `top + dy`.
- a zoom settle changes `s`: every position scales, and the class of every text layer carries
  `font-size: {layer.font_size * s}px`, `letter-spacing`, the halo width and `transform-origin` in px: the
  render's sheet differs, every label is cascaded and shaped anew. Nothing of a zoom settle can be kept while
  the labels are laid out in px of the zoom (that would need labels laid out once in map units and scaled by
  the picture's transform, which changes how text is rasterised: not proposed here).
- consequence for azul: on a pan settle every label is "moved in place" (its own `left` / `top` values and
  nothing else): no label is cascaded (diff-first-11), the solver keeps its box and places it again, and the
  list paints it anew (diff-first-15: a moved box is painted by any pass that is no tween frame). The step
  `list` copies nothing there.

**What the editor must change so that a pan settle keeps the insets of the labels that stay** (the picture
is anchored, the view's offset carries the pan):
1. keep an ANCHOR in `ViewMotion`: the scale and pan the picture's coordinates are relative to, e.g.
   `anchor: Option<(f32, (f32, f32))>`. Every position the render writes is
   `((x_mm + anchor.pan.x) * anchor.scale, (y_mm + anchor.pan.y) * anchor.scale)`: a function of the map
   position and the anchor alone, so bit for bit the value of the render before for every node that stays.
   (Not `to_screen(..) - shift`: `pan` accumulates `dx / s` per step and `shift` the unrounded sum of `dx`;
   their difference is not bit-stable, and a value one ulp off is a moved label.)
2. a settle render after a pan at the anchor's scale KEEPS the anchor and returns the view's content offset
   instead of resetting it: `offset = ((pan - anchor.pan) * s)` rounded to whole device pixels (as `pan_view`
   rounds its shift today), handed back as the origin of the render's `materialized` rect - what
   `update_virtual_view` writes during the pan. The sub-pixel rest stays out until the next rebase: a picture
   up to half a device pixel off the model, as it is during the pan today.
3. REBASE (anchor = the model's scale and pan, offset = 0: every node moves once) when the scale changed (a
   zoom settle), when the offset leaves a bound (say 8,192 px: positions stay exact to 1/1024 px in an
   `f32`), and on the first render.
4. what depends on the view's rectangle follows the offset: the ground box (`-o, -o, size + 2 o`) is placed
   at `offset - o`; `near_view` and `visible_tiles` test screen positions as now (they pick WHICH labels and
   tiles are rendered); the tiles' edges are rounded in picture coordinates (the offset is whole device
   pixels, so their seams stay closed); the last key's message (`top: size.height - 34`) and the hover
   highlight are placed at `offset + ...`; `write_picture_transform` and `zoom_state` already carry `offset`.
5. `motion.rendered` (what a zoom's transform starts from) becomes the anchor plus the offset.
What it saves on a pan settle, per label that stays (est., the 12,500-node view, 95 % staying):
- the cascade's part is saved already (no label is cascaded when only its insets move);
- the compact entries of each moved label, its fingerprint in the reconcile and its placement
  (`position_out_of_flow_elements`): est. 1-3 us per label, 12-35 ms per settle today, down to the 5 % that
  came;
- the list: est. 12-25 ms down to 1.5-3 ms of copies plus 0.6-1.3 ms for the labels that came - but only
  once the list is patched ACROSS A CHANGED TREE (diff-first-07) and the solver keeps renumbered boxes
  (diff-first-06b): nearly every pan settle brings labels in and takes labels out, so its tree changes.
  With the step `list` alone the gain is there only for a settle that changes no node of the tree.
Until the editor is anchored, diff-first-07 and -06b cannot show a gain on a pan settle either: all labels
move.

## For area dom-builder

Checked against `regenerate_layout` at this branch's tip (after the steps `tree` and `view`), for the two lines
of `audit/dom-builder.md`, "## For area diff-first" (1):

**The two lines stand as written.** Both sites are unchanged here: step 1.35 still reads
`let _ = layout_window.resolve_form_controls(&mut user_dom);` and the early diff
`Some(azul_core::diff::fingerprint_dom(&user_dom))`, with `user_dom` a `let mut`. `DomFingerprints` has the seven
fields it had at `cfc4d6d72` (no tier was added), so `prev != Some(&out.0)` in `fingerprint_built_dom_against`
compares what it compared.

**One thing the merge has to place by hand, or a flat DOM reaches a tree walk:** the note's reason "the only
later reader of `user_dom` is `style_user_dom_for`, which expands it" has a second reader here. When the `Dom`
stays flat (fingerprints equal) and exit A's gate still fails on the node count (the retained arena is not the
shape of the fingerprints: a generated node, a resolved icon), exit 1.47 reads it:
`layout_window.flatten_user_dom_for(user_dom, current_window_state)`. And the body `dom.expand_flat()` was added to
(`LayoutWindow::style_user_dom_inheriting`, dom-builder `layout/src/window.rs` 25435) has MOVED on this branch:
`style_user_dom_inheriting` is now `flatten_user_dom_inheriting(..)` + `StyledDom::cascade_at_rest()`, and its
parameter is `dom: Dom` (no `mut`). The merge will conflict there. Resolve it so that the line is the first
statement of `flatten_user_dom_inheriting`:
```
    pub fn flatten_user_dom_inheriting(
        &self,
        mut dom: Dom,
        window_state: &FullWindowState,
        form_scope: u64,
        inherited_from_host: azul_core::prop_cache::InheritedFromHost,
    ) -> StyledDom {
        // A DOM its builder handed over flat (`azul_core::dom::DomBuilder`) is
        // a tree from here on: every pass below walks one.
        dom.expand_flat();
        // The form controls are widgets built HERE, after `layout()`
```
and `style_user_dom_inheriting` stays the two-line wrapper it is here. Then every way to a styled DOM expands:
`style_user_dom_for` / `style_user_dom_inheriting` (through the wrapper), `flatten_user_dom_for` (exit 1.47) and
the view's re-render under the step `view` (which calls `flatten_user_dom_inheriting` directly; its site's
`fingerprint_built_dom(&dom)` is unchanged here).

**What the steps `tree` and `view` read of the builder's fingerprints**, beyond what the first half read:
`structure.len()` (must be the arena's node count for any row to be kept) and `sheets` (the hash of the sheets hung
on each node, by pre-order index; `StyledDom::cascade_reusing` compares the old build's entry of a node with the
new build's). A builder whose `sheets` differ from `fingerprint_dom`'s for the same tree makes the cascade keep
no row of that node (correct, slow), or - if two different sheets hashed alike - keep a row it must not: the
builder's `hash_sheets` has to stay the copy of `node_sheets_hash` it is
(`a_ui_built_both_ways_is_the_same_tree_with_the_same_fingerprints` on that branch says so).

The note's (2), (3), (5) (`nodes_at` for a flat `Dom`, `fingerprint_dom` itself flat-aware, a view's changed
rebuild reading `flat_node_data_at`) touch nothing these steps changed: exit 1.47 and the view's step read the
ARENA after the flatten, never `nodes_at`.

## The four rebuild cases after the steps `list`, `wlist`, `tlist` (estimates by reading, not measured)

Per 1,000 nodes: cascade 2-6 ms, emitting a list 1.0-2.0 ms, copying it 0.1-0.25 ms, a box laid out anew
0.3-1 ms where its text is in the shaping cache.

| rebuild | before this session | now | what is left, and whose |
|---|---|---|---|
| identical (the window's DOM, about 4,000 nodes) | no cascade, no pass, the list kept | the same | the `Dom` build, its fingerprint walk and its drop (diff-first-09: 0.6-2.4 us a node) |
| nodes changed where they stand (one class of 4,000 nodes; exit 1.46) | cascade for the reached nodes; list emitted in full: 4-8 ms | list patched: 0.4-1 ms of copies plus the reached boxes (`wlist`) | the O(n) walks of diff-first-08 (0.3-0.6 ms); a change that moves the boxes after it paints them |
| 500 nodes come among 3,500 (exit 1.47) | cascade for the 500; every box after the first new node laid out anew; list in full: 4-8 ms | at the END of the page: the 3,500 before are copied, the 500 painted: about 1 ms of list (`tlist`). In the MIDDLE: the boxes before the first new node copied, every box after it fresh and painted | diff-first-06b (the solver across renumbering): until then a node that comes early costs every later box its layout and its paint; `reconcile_dom`'s hashes (diff-first-05: 2.4-4 ms) |
| a view rendered again (the editor's settle, 12,500 nodes) | cascade for what entered; every label that moved placed again; list in full: 12-25 ms | the tree and the insets kept (a selection, a hand edit): 1.5-3 ms of copies (`list`). A pan or zoom settle: unchanged, because the editor gives every label new insets (all painted) | the editor's anchor ("For area A7-editor"), then diff-first-06b for labels entering in the middle of the list |

The user's 1-2 ms for a full rebuild is reached for the identical rebuild and, by these estimates, for one
changed class and for nodes appended; not for a node that comes early (06b) and not for the editor's settle
(the anchor, then 06b).

## Hand-over (kept current at every commit)

**Built (branch `audit/diff-first`, all `[unverified]`):**
| what | commit | tests to run |
|---|---|---|
| first half: steps `text`, `nodes`, `style` | 3d51fc7fa, 03af4217d, 5acd151cf, 009e311d8, 00b4d8305 | core `diff_test::audit_diff_first_tests`, `styled_dom_test::audit_diff_first_tests`; dll `headless::tests::a_rebuild_*`, `one_changed_*`, `an_identical_rebuild_*` |
| diff-first-10 (full path, inherited hover value) | f589b3743 | dll `a_full_rebuild_keeps_the_hover_colour_the_hovered_row_hands_its_text` |
| rows agent 1 (step 3.4's line) | 8cfbd5812 | dll `rows_that_come_are_styled_and_laid_out_and_no_other_node` |
| step `tree`, core (`unstyled_*`, `cascade_at_rest`, `spliced`, `cascade_reusing`) | 06480341b | core `styled_dom_test::audit_diff_first_tests::a_cascade_that_keeps_rows_*`, `sections_that_come_*`, `a_node_written_*`, `a_node_whose_own_sheet_*`, `an_unstyled_dom_*` |
| step `tree`, window and shell (exit 1.47, the solver's flags) | e332a6b34 | dll `a_rebuild_that_changed_the_tree_*`, `rows_that_come_*`, `five_hundred_nodes_*`, `the_steps_of_the_switch_*`, `a_changed_text_takes_the_full_path_*` |
| step `view`, the pairing path (+ the class "moved in place") | 43bb97f38 | dll `a_view_rendered_again_*`, `a_view_whose_labels_all_moved_*`, `a_view_that_exchanged_5_percent_*` |
| step `view`, the kept exits (`restyle_kept_child`) | eba4d13e4 | the three above + dll `one_changed_label_of_a_view_styles_that_label` |

| diff-first-13 (a written DOM gets no clean flags; the slider) | 6c43652f1 | dll `dragging_the_slider_leaves_no_thumb_behind`, `a_style_a_callback_wrote_is_laid_out_by_the_rebuild_it_asked_for` |
| diff-first-14 (the gate `animation`, per reached node) | 6e4391abc | dll `an_animated_node_elsewhere_does_not_send_a_rebuild_down_the_full_path`, `an_identical_rebuild_and_a_changed_text_style_no_node` |
| diff-first-15 (a moved box is painted anew by a patched pass; `AZ_DL_TRANSLATE_MOVED_RUNS`) | 8447aa202 | layout `audit_tests::diff_first::*` (three), `audit_tests::a3_window::a_box_that_moved_in_a_pass_is_painted_as_a_fresh_layout_paints_it`; dll the five walks `a_rebuild_down_the_diff_first_path_leaves_what_the_full_path_leaves`, `a_rebuild_that_changed_the_tree_*`, `a_view_rendered_again_leaves_what_the_full_path_leaves` |
| diff-first-15, second part (the frames of a layout tween translate and record; the last frame and every other pass paint) | bf43a5e96 | layout `audit_tests::diff_first::*` (seven); `cargo test -p azul-layout --test all a_growing_ribbon` and `a_height_tween` (the two guards, not edited); layout `a_layout_tween_frame_reuses_the_tree_and_patches_the_list` |
| diff-first-19 (a state that hands a value down no longer costs the DOM its inherited UA values) | 8e9ead705 | core `audit_diff_first_tests::a_state_that_hands_a_colour_down_costs_no_other_node_what_it_inherits`, the whole core suite (hover / focus restyles); dll the three `*_under_the_pointer_*` walks, `a_full_rebuild_keeps_the_hover_colour_the_hovered_row_hands_its_text` |
| diff-first-19, second part (what a node hands down from a state beats its children's copy of the resting value; both stores agree) | 3afb0f554 | core `audit_diff_first_tests::a_state_that_hands_a_colour_down_costs_no_other_node_what_it_inherits`, `a_state_hands_down_a_property_whose_copy_survives_the_prune`, the whole core lib and `--tests` (`css_inheritance.rs` calls the pass directly); dll the three `*_under_the_pointer_*` walks; layout `a3_window::a_hover_colour_patched_into_the_list_is_the_list_a_build_gives` |
| diff-first-18 part 1, step `list` (a view's re-render hands the pass the nodes it cascaded; the list is patched) | aa929c920 | dll `one_changed_label_of_a_view_paints_that_label_and_copies_the_rest`, `a_view_that_restyled_5_percent_of_its_labels_paints_5_percent`, `a_view_whose_labels_all_moved_paints_every_label`, `a_node_the_cascade_reached_is_painted_though_its_box_is_clean`, `a_view_rendered_again_leaves_what_the_full_path_leaves`, `the_steps_of_the_switch_are_named_one_by_one`; the layout lib (views, damage: nothing of it was read for a pinned full emission) |
| diff-first-20, step `wlist` (exit 1.46 hands the pass its cascaded nodes) | 554d8ccd2 | dll `one_changed_class_paints_its_row_and_copies_the_windows_list`, `a_box_the_windows_cascade_reached_is_painted_though_it_is_clean`, the `df` walks (`a_rebuild_that_styled_its_changed_nodes_*`, under the pointer too) |
| diff-first-07, step `tlist` (the list patched across a changed tree; exit 1.47 hands its names) | dd0ef31d4 | dll `rows_that_come_are_painted_and_the_rows_before_are_copied`, `a_box_that_got_another_layout_index_is_copied_from_the_run_it_had`, `labels_that_enter_a_view_are_painted_and_the_others_copied`, every `df` / `dfv` walk and count test (the helpers and both fixtures changed), `the_steps_of_the_switch_are_named_one_by_one` |
| diff-first-15, the window area's test (its own mutant, its premises) | 55c008027 | layout `audit_tests::a3_window::a_box_that_moved_in_a_pass_is_painted_as_a_fresh_layout_paints_it` |
| diff-first-17 (the core fixtures: a node's own style, not `Dom::with_css`) | 8420ec290 (`diff_test.rs`), 8e7a62d2c (`styled_dom_test.rs`) | core `diff::diff_test::audit_diff_first_tests` (all), `styled_dom::...::audit_diff_first_tests::the_cascade_over_changed_nodes_*` (three walks) and the rest of that module |

**The verifier's first run of the dll suite (integration `57a46db75`, first half): the seven of this area.**
1. `dragging_the_slider_leaves_no_thumb_behind`: a real defect of the kept exits: fixed 6c43652f1 (diff-first-13).
2. `a_changed_text_takes_the_full_path_where_the_reconciliation_has_work` ("a handler: the full path"): the
   expectation was the step `text`'s and the test ran with every step: fixed in e332a6b34 (the test sets `text`).
3. five equality walks, one f32 item in the last bit: the solver's display-list patch translated a copied run by
   `new - old`; not a function of these steps. Fixed here by the coordinator's decision: diff-first-15.

**Most likely to fail compilation or to draw wrongly, first:** eba4d13e4 (the arm of
`invoke_virtual_view_callback_impl` gained a branch without re-indenting what it wraps; partial moves out of the
taken result) > e332a6b34 (the shell's control flow around the
unstyled tree; `begin_reconciliation_before_styling`'s borrow of the old result) > 43bb97f38
(`reconcile_rematerialized_child` re-borrows the new arena after the cascade; the test closures of `dfv_mutate`)
> 06480341b (`cascade_at_rest` on `self`: the split of `create_from_compact_dom`; `spliced`'s struct update) >
f589b3743 > 8cfbd5812. If the split of the creation is wrong, EVERY window is affected (it is on the full path
too): `an_unstyled_dom_cascaded_at_rest_is_the_dom_the_creation_makes` and the whole existing core suite guard it.
After those: 6e4391abc (`capture_declared_transitions` is `begin_reconciliation`'s transition code a second time,
with remapped ids: a wrong start value shows as a jump at the first frame of a transition after a rebuild that
changed the tree) > diff-first-15 (four lines and two free functions: little to get wrong in the build; what it
changes is the COST of every patched pass that moves boxes, see its block; its three tests stand on a resize that
moves boxes anchored right / bottom and behind a percentage margin, and their premises say so if neither moves)
> its second part (a new field on `DisplayList`: four struct literals without `..Default::default()` were
extended, three in `display_list.rs`, one in `cpurender/compositor.rs`; a merge that brings a fifth fails to
compile and wants `translated_since_painted: std::collections::BTreeSet::new()`; the stamp's comparison in
`layout_document` is `describes` now: if a tween frame stops taking the retained tree, look there; the tween
tests stand on `background-clip: content-box` painting a rect at `position + padding` and on a `NodeCss` write
of `height` seeding a tween, as the tests of `layout/tests` do).

**The verifier's run on `8e7a62d2c`** (told by the coordinator): compiled with one test-only fix
(`styled_dom_test.rs`: `.style` moved out of a `NodeData`, which implements `Drop`: `.style.clone()`); core lib
3,143 / 0 (all `audit_diff_first_tests` green: the cascade over a changed own style holds), layout lib 10,906 / 0,
the slider and the plain walks green, the tests of the steps `tree` and `view` green. Red: the ribbon guard (as
expected on `8447aa202`: fixed by bf43a5e96) and the three walks under the pointer (diff-first-19).

**In flight (third successor, 2026-11-21):** nothing uncommitted. The branch holds `audit/integration` up to
`a10556742` (merged twice at the start). Done by this agent: diff-first-19 second part (`3afb0f554`),
diff-first-18 part 1 = step `list` (`aa929c920`), the answer for the editor ("For area A7-editor (m4p)"),
diff-first-20 = step `wlist` (`554d8ccd2`), diff-first-07 = step `tlist` (`dd0ef31d4`).

**By reading, not settled (the verifier's run decides; each is a premise assertion in its test):**
- whether the epochs of a `CascadedSince` survive from the hand-over to `layout_document` in the common case
  (`patched == 1` / `across == 1`); if not, look for what moves the DOM's `cascade_epoch` in between, or for a
  list that was not built at the old DOM's epoch;
- the run counts (`copied >= ..`, `painted in ..`): estimates of how many paint calls a row or a label makes;
- `a_box_that_got_another_layout_index_is_copied_from_the_run_it_had`: that a `display: list-item` row keeps
  its height (premise "the rows after the list item did not move");
- whether a layout or dll test pins a view's re-render or a window rebuild as a FULL emission
  (`last_build_was_patched == false`, a damage list): such a rebuild is a patched build now.

**Left, each with its design in the block named:**
1. The solver across renumbering: diff-first-06b (A3-solver's and A8's files): what is left of every case
   where a node comes or goes before others.
2. Gates: the table above. Open with a design and no code: `+` / `~` on exit 1.46 (the later siblings only),
   the drag state (rows agent 2).
3. The rows agent's changes 2-4 (above).
4. diff-first-05 (the hashes and maps of `reconcile_dom`), diff-first-08 (tags, the cache clone of exit 1.46,
   the animation lookup), diff-first-12.
(Done: the window area's test `a_box_that_moved_in_a_pass_is_painted_as_a_fresh_layout_paints_it` runs its
own mutant and asserts its premises, `55c008027`. If its first premise fails - no run translated under
`AlwaysTranslate` - the `NodeCss` pass of that page builds its list in full and the page needs a pass that
patches: a resize, as `audit_tests/diff_first.rs` has.)

## Not read

- `layout/src/solver3/*`: read `mod.rs` 840-1000 (the reconcile's entry, the skips) and `cache.rs` 1985-2440,
  3290-3700 (`reconcile_and_invalidate_restyled`, `DomPairing`, `DomDiff`, `reconcile_node`); nothing else.
  `display_list.rs`: `PatchState`, `try_copy_cached_run`, `translate_item`, `compute_patch_move_summary`, the
  two callers of the copy; `mod.rs` 2095-2425 (the patch gate, the move summary's inputs).
  `cpurender/compositor.rs`: `translate_hint_for_patch`, `items_equal_translated` and the move arm of
  `compute_display_list_damage_impl` (5939-5965) only.
- `core/src/prop_cache.rs` 3600-6050 only at the functions named in this file; `restyle_cascade` 2237-2945 at
  the fields it writes and its rule classification (2360-2570), not its inheritance walk (the copy of it in
  `recascade_nodes` was read in full); the `calc_*` and scrollbar getters not at all.
- `css/src/compact_cache.rs`: the struct, `with_capacity` and the px-overflow accessors; not the slot codecs.
  `core/src/compact.rs`: `build_compact_rows` 583-660, 846-1110, 1195-1396 (what a row is built from).
- `core/src/custom_property_cascade.rs`, `generated_content.rs`, `ua_css.rs`: the entry predicates only.
- `remap_node_ids`, `finish_reconciliation` (its head only), `layout_dom_recursive_with_viewport`,
  `follow_host_in_kept_view` (its head), the view manager.
- `text3/**`: nothing (06b needs it).
