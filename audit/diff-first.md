# diff-first: the rebuild pipeline that diffs before it styles

Task of the 2026-11-21 audit (RULES.md 8.8). Blind: nothing here was compiled or run.
Branch `audit/diff-first`, cut from the tip of `audit/A3-window` (`4d7cabccc`), `audit/integration` merged once
(`21ff69a5e`, after diff-first-02). The trace this builds on is `audit/A3-window.md`, "Why a full DOM rebuild is
slow" (the 27 stages) and "The diff-first pipeline" (step 1, `dd25f38e7`). `layout.rs` =
`dll/src/desktop/shell2/common/layout.rs`.

**State in one paragraph.** A rebuild that keeps the TREE (same nodes in the same places, same attached sheets) no
longer takes the full path, whatever it changed in its nodes: texts (step 1, the predecessor's), classes, ids,
attributes, inline styles, flags (diff-first-02), and the cascade then runs only for the changed nodes and what they
reach (diff-first-03). A rebuild that CHANGES the tree (a node added, removed, moved) still takes the full path:
the design for it is complete below (diff-first-04..07), with what the code forced to change in the predecessor's
design; it is not built, for the reasons given there.

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

Not touched (the parallel A3-window agent's): `node_patch.rs`, `apply_node_style_change`, `tick_animations`.

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

The three cases of the trace, est. total in azul after the app's callback, at 4,000 nodes:
- **identical rebuild**: 2.5-7 ms, unchanged (it was exit A already): the callback's build, the fingerprint and the
  drop are all of it. Count test: `an_identical_rebuild_and_a_changed_text_style_no_node` (0 nodes styled, 0 layout
  passes, the list where it lay).
- **one text changed**: the same 2.5-7 ms + the text's layout (was 16-40 ms; step 1).
- **one class changed** (not in the trace's three, the case this task adds): the same + 3-7 styled nodes + their
  layout + about 0.3-0.6 ms of O(n) remnants (diff-first-08); was 16-40 ms.
- **500 nodes appearing among 3,500**: 25-70 ms, UNCHANGED: the full path (diff-first-04..07, designed).

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
- file:line: `core/src/styled_dom.rs` `create_from_compact_dom`; `core/src/prop_cache.rs`; `layout.rs` 1054-1300
- class: waste, per rebuild that adds, removes or moves a node: stage 10 for all n nodes
- design (complete; what the code forced beyond the predecessor's step 4):
  1. THE MAP. Styling reuse needs "new node n has the rows old node o had", which is NOT identity (state follows
     `reconcile_dom`; rows follow content and context). So the map comes from the fingerprints alone and never has
     to agree with `reconcile_dom`: add `own: Vec<u64>` (the node hash without the child count; `structure` =
     fold(own, child count)) and `subtree_hash: Vec<u64>` (fold of `own`, `style` over the subtree, on the way back
     up the walk); align the children of two paired parents by longest common subsequence over `subtree_hash`
     (children lists are short; for a list of thousands: common prefix and suffix, then the middle as new). A pair
     with equal `subtree_hash` maps its whole subtree by offset.
  2. CLEAN (n maps to o, both under clean parents that map to each other) needs, beyond equal `own` + `style`:
     (a) `CascadeInfo` equal on both sides (compared directly: `old.cascade_info[o] == html_tree[n]`), only when a
     rule of the new sheet has a structural pseudo-class (one flag per cascade, computed where `left_state_mask`
     is); (b) under a sheet with `+` / `~`: the previous element sibling clean and mapped to o's previous one;
     (c) same window context, same `inherited_from_host`, same user sheets, same `*` bucket
     (`recascade_nodes` already compares the bucket); (d) no user override on o or on an ancestor of o for an
     inheritable type (`user_overridden_properties`: an override is baked into o's compact entries and computed
     values, and follows IDENTITY through `migrate_user_overrides_from`, not this map); (e) the sheets in scope
     are the same sheets on the same nodes: every ancestor's `sheets` entry equal (they are part of `style`).
     A container that gained a child stays clean (its `own` hash is unchanged): this is what makes the appended
     panel cost its own nodes.
  3. THE SPLICE. `CssPropertyCache::spliced(&old, old_of_new) -> CssPropertyCache`: every per-node table re-indexed
     for the new arena. `FlatVecVec`: `offsets[n] = old.offsets[o]`, the data shared by clone (it is deduplicated:
     tens of runs). `InheritedValues`: every bucket's node list mapped through `new_of_old` and sorted.
     `CompactLayoutCache`: the four row vectors gathered by `o`, `px_overflow` and `prev_font_hashes` re-keyed.
     `inline_overruled`, `css_wide_values`, `env_readers.nodes`: re-keyed. `user_overridden_properties`: EMPTY (a
     new DOM has none; the reconciliation migrates them). Then `recascade_nodes(new sheet, .., dirty)` with every
     unmapped or not-clean node dirty: the function as built, unchanged (its ancestor-chain rebuild already covers
     a new node under clean ancestors).
  4. THE STATES. The full path cascades the new DOM AT REST and raises the states after the reconciliation
     (stage 14), with a second whole cascade when a raised state is named left of a subject. The kept rows were
     cascaded under the OLD DOM's states. So either (i) reuse only when no state the old sheet names left of a
     subject is raised on the old DOM (`left_state_mask & raised == 0`: wrong for the common "the pointer is over
     the button that caused the rebuild" under a widget theme), or (ii) reorder the full path: flatten ->
     `reconcile_dom` on the flat arenas (it needs node data and hierarchy only) -> `remap_node_ids` -> raise the
     states on the new arena -> ONE cascade with reuse under them -> the rest of `begin_reconciliation` (CSS diff,
     transitions, zombies). (ii) is the predecessor's "the states are written first and the cascade runs once",
     and it ends stage 14's second cascade; it splits `begin_reconciliation` in two and moves `style_user_dom_for`
     between the halves. An inheritable declaration of a raised state is the one place where (ii) changes what
     the full path computes: see diff-first-10.
- why not built: (ii) is a re-architecture of the shell's rebuild and of `begin_reconciliation` (680 lines) that
  cannot be proven by reading in one session on top of three new steps; (i) alone would be off whenever the pointer
  is over a themed widget. The splice (3) and the map (1) are each about a day's work and independently testable
  against a from-scratch cascade with the core harness of diff-first-03 (`differences`).
- severity: high
- status: blocked (a): a re-architecture of the order of reconciliation and styling on the full path (point 4 ii); the design is this block; built short of it: the unit of reuse with its proof and `recascade_nodes`, which the splice feeds unchanged

### diff-first-05 reconciliation fed by the early diff; the arena copy and the maps of `reconcile_dom`
- file:line: `layout/src/window.rs` `begin_reconciliation` (the copy at `reconcile_arena_copy`); `core/src/diff.rs`
  `reconcile_dom`
- class: waste, per rebuild on the full path: n x 1,200-2,500 ns
- what is done: on the three kept-DOM exits neither runs (no copy, no map, no hash).
- what is left, for a changed tree: `reconcile_dom` is the definition of identity there and its passes are global
  (A2's "unique in its container on both sides" counts every node), so no shortcut for unchanged subtrees is
  semantics-preserving in general. Sound reductions, in order of gain: (1) the deep copy of the old arena:
  `transfer_states` needs `&mut` old nodes only for nodes with a merge callback (it TAKES the old dataset), the
  unmount / focus-lost queues read old nodes with handlers: keep `old_node_data` as a borrow of the retained arena
  for the diff and clone only the unmatched old nodes that have a callback plus the datasets a merge takes (the
  code already clones those: `taken`); needs `begin_reconciliation` to hold the old `DomLayoutResult` out of
  `layout_results` for its duration (it reads `self.layout_results.get(&dom_id)` in four more places). (2) the
  two `compute_subtree_hashes` (a full `NodeData::hash` per node per side): keep the old side's from the last
  rebuild next to the fingerprints (8 bytes per node). (3) `old_by_rec_key` and `old_by_subtree`: a
  `VecDeque` allocation per distinct key; one sorted `Vec<(key, node)>` each.
- severity: medium
- status: open: designed in this block (three reductions, each local to one function), not built: the session's remaining room went to the proof and tests of diff-first-03

### diff-first-06 the solver fingerprints every node when the node count changed
- file:line: `layout.rs` "GRANULAR DIFF" (sets `dom_diff_clean` only for equal counts); `layout/src/solver3/cache.rs`
  `reconcile_and_invalidate_restyled`
- class: waste, per rebuild that changes the tree: n x 200-500 ns
- design: after `begin_reconciliation` the pairing exists (`node_moves`; `LayoutCache::dom_pairing` is filled by
  `remap_node_ids`). `clean[new] = paired with old && structure, style hashes equal at (old, new) && parent clean
  and paired with old's parent`, computed from `prev_dom_fingerprints` / the new fingerprints through `node_moves`
  (pre-order indices are arena indices while no node is generated or injected: the same count gate as today).
  The solver's reader has to accept the flags for a pass whose tree changed: `reconcile_and_invalidate_restyled`
  was NOT read by this session (A3-solver's file), which is why this is not built.
- severity: medium
- status: open: the shell side is ten lines at "GRANULAR DIFF" (given above); the solver's acceptance of `dom_diff_clean` beside `dom_pairing` has to be read first (`cache.rs`, not read)

### diff-first-07 the display list is emitted for every node when one node comes or goes
- file:line: `layout/src/solver3/display_list.rs` `PatchState` (refused when `last_reconcile_structure_preserved`
  is false)
- class: waste, per rebuild that changes the tree: every item emitted, a new list for both renderers
- design: the predecessor's step 7 stands as written (`audit/A3-window.md`): with the pairing, the old layout index
  of a clean node is `old.dom_to_layout[old_of_new[n]]`; the builder copies the run of a clean subtree whose boxes
  did not move (`translate_item` for one that moved as a whole) and emits for the rest. Not read by this session
  beyond `PatchState`'s refusal; A3-core's notes list what of `display_list.rs` was read.
- severity: high
- status: blocked (a): needs the old-to-new box mapping through a changed layout tree inside the display-list builder (20k lines, not read in full by any session); the design is the predecessor's step 7

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
- file:line: `layout.rs` `apply_runtime_states_before_layout` -> `StyledDom::restyle_for_raised_states`
  (`core/src/styled_dom.rs`)
- class: wrong-output (until the next hover change), on the FULL path, found while proving equality
- evidence: a full rebuild cascades the new DOM at rest (`compute_inherited_values(.., &[])`), then raises the
  runtime states flag by flag. `restyle_for_raised_states` runs the cascade again only when a raised state is named
  LEFT of a subject. A node that is hovered and declares an INHERITED property for `:hover` (`.row:hover { color:
  .. }`) therefore shows its own hover colour (the lookup reads its state) while its children keep the resting
  colour (their computed values and compact slots were made without the parent's state overlay), until the pointer
  leaves and enters again (`restyle_nodes_state` recomputes inheritance for states). The kept-DOM exits do not have
  the defect: the kept rows hold the overlay, and `recascade_nodes` applies it for a kept parent. So the two paths
  DIFFER for such a sheet, and the diff-first one is the right one. The equality walks of the dll use a hover rule
  with a non-inherited property for that reason; the core walk `_under_a_hover` covers the inherited one against
  `StyledDom::restyle`, which does apply overlays.
- fix (not applied: it changes what the full path draws, and belongs with stage 14's rework in diff-first-04 point
  4): in `apply_runtime_states_before_layout`, when `restyle_for_raised_states` returned false and any raised node's
  `state_inherited_values` is non-empty, call `styled_dom.recompute_inheritance_for_states()`.
- severity: medium
- status: open: the two-line fix is given above; not applied because nothing in this branch can show its effect without a compiler (it changes the reference path of every equality test here)

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
  - `text`: exit 1.45 (texts alone). Off: such a rebuild takes the full path (also when `nodes` is on).
  - `nodes`: exit 1.46 (nodes changed where they stand; without `style` it runs the whole cascade in place).
  - `style`: inside `nodes`, the cascade over the reached nodes only.
  Examples: `AZ_DIFF_FIRST=text` = the state before this task; `AZ_DIFF_FIRST=text,nodes` = this task without
  `recascade_nodes`.
A test picks its steps per thread: `window::set_diff_first_steps_on_this_thread(Some(&[..]))`,
`set_diff_first_on_this_thread(Some(bool))`. Which path a rebuild took: `window::kept_dom_rebuilds_on_this_thread()`
(exit 1.46), `styled_dom::nodes_cascaded_on_this_thread()`, `styled_dom::compact_cache_rebuilds_on_this_thread()`.
The pre-cascade exit for an identical rebuild is older than the switch and not under it.

## api.json

No change. Everything added is Rust-internal (`azul_core::diff`, `azul_core::styled_dom`, `azul_layout::window`).
Proposed and NOT made: `DomBuilder` / a layout callback that may return `FastDom` (diff-first-09).

## Probe spans (`AZ_PROFILE=cpu`)

Added: `regen_nodes_changed` (the whole exit 1.46), inside it `regen_restyle_changed_nodes` (the cascade: over the
reached nodes, or whole) and `regen_css_diff_changed_nodes`. The predecessor's `regen_texts_only`,
`regen_fingerprint_dom`, `regen_precascade_exit`, `regen_style_user_dom`, `regen_begin_reconciliation` (...) stand.
A rebuild shows exactly one of `regen_precascade_exit`, `regen_texts_only`, `regen_nodes_changed`,
`regen_style_user_dom`: which exit it took. azul-core has no `Probe`: the stages inside `recascade_nodes` are one
span (the counter `nodes_cascaded_on_this_thread` says how many nodes it ran for).

## What to look at on the real window

Default ON for all three steps, because each exit falls back to the full path by itself wherever a gate fails, and
`AZ_DIFF_FIRST=0` gives the reference to compare with.
- Toggle something that changes a class or an inline style with a `RefreshDom` (select a row, open a disclosure
  that only restyles, move a label by its style): the changed node shows its new style, its children too (an
  inherited colour or font size), nodes beside it do not move unless the layout says so. With `AZ_DIFF_FIRST=0` the
  same clicks must look the same.
- With the pointer resting on a button that has a hover face while the rebuild happens: the hover face stays.
- A focused text field in a page whose other nodes change class: the caret stays, typing continues.
- A list whose rows exchange their texts on a rebuild (a sort without keys) while one row is hovered or focused:
  with `AZ_DIFF_FIRST=0` the state follows the text, by default it stays at the place ("What unchanged means",
  identity). If an app relies on the first, it should key its rows (`with_key`).
- `AZ_PROFILE=cpu`: a class toggle shows `regen_nodes_changed` and no `regen_style_user_dom` /
  `regen_begin_reconciliation`.

## What the predecessor's design got wrong (found by reading the styling code in full)

1. "`adopt_rows` copies the row of every per-node table": the tables are not rows. `css_props` / `cascaded_props`
   are flat stores with shared runs, `computed_values` is transposed (a node list per distinct value), and the
   kept `cascaded_props` rows are PRUNED - a dirty child cannot inherit from them. Hence the ancestor-chain rebuild.
2. "the tag id KEPT": tags are `index + 1`, nothing to keep.
3. "an unchanged NODE" as `structure` + `style` equal: `structure` holds the child count, so a container that
   gains a child is "changed" and its whole subtree with it. The unit needs the node hash without the count
   (`own`, diff-first-04).
4. (b) asked the sibling condition of every ancestor: only where a rule can read it (`+`, `~`, a structural
   pseudo-class); and `:nth-last-child`, `:only-child`, `:empty` do not exist in this engine.
5. Step 3's `pair_builds` "replaces `reconcile_dom`": it cannot - `reconcile_dom` pairs by `NodeData::hash`
   content first (pass A2) and by children (A3), and no pairing over the fingerprint tiers reproduces it. The
   styling map and the identity pairing are two different relations; only the first can come from the
   fingerprints (diff-first-04, point 1).
6. The computed values of a node depend on the STATE of its parent (state overlays), not only on left-of-subject
   rules: condition (e) of the design was too narrow. See diff-first-10.
7. Step 2 ("k times `set_node_style`, the cascade's tail once") is subsumed: an inline style is one of the node
   changes of exit 1.46, and `recascade_nodes` runs the tail for the reached nodes only.

## Next (for whoever continues)

1. The same three exits for a VIEW's child DOM (`reconcile_rematerialized_child`, beside
   `reclaim_unchanged_child`; the child's fingerprints are in `child_dom_fingerprints`): the settle of a map whose
   labels only moved is `same_tree_changes` with every span named - more than half of the nodes, so the whole
   cascade in place, but no flatten, no `reconcile_dom`, no arena copies. Needs `css_diff_of_kept_dom` for the
   child (it exists) and the child's `layout_cache`.
2. diff-first-04 in its order: `own` / `subtree_hash` tiers and the map (testable alone), the splice (testable
   against a from-scratch cascade with `differences` of the core tests), then the reorder of the full path.
3. diff-first-06 after reading `reconcile_and_invalidate_restyled`; diff-first-05 (1); diff-first-08 (1)-(4).

## Not read

- `layout/src/solver3/*` (only `LayoutCache`'s fields named here); `display_list.rs` beyond `PatchState`'s gate.
- `core/src/prop_cache.rs` 3600-6050 only at the functions named in this file (`get_declared_property_ranked`,
  `inline_properties`, `declares`); the `calc_*` and scrollbar getters not at all; `css_wide_values_of_node`,
  `resolve_*` by signature only.
- `css/src/compact_cache.rs`: the struct, `with_capacity` and the px-overflow accessors; not the slot codecs.
- `core/src/custom_property_cascade.rs`, `generated_content.rs`, `ua_css.rs`: the entry predicates only.
- `reconcile_rematerialized_child`, `remap_node_ids`, `finish_reconciliation`: not read (the kept-DOM exits do not
  reach them; the predecessor's notes describe them).
- `audit/A3-solver.md`: the three answers and the table; the code it names was not read.
