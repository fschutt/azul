# dom-builder: a `Dom` builder that produces the flat, single-allocation form

Task of the 2026-11-21 audit (RULES.md 8.8; `audit/diff-first.md`, block diff-first-09). Blind: nothing here was
compiled or run. Branch `audit/dom-builder`, cut from the tip of `audit/diff-first` (`cfc4d6d72`), `audit/integration`
merged at the start (`a03aed60c`). `layout.rs` = `dll/src/desktop/shell2/common/layout.rs`, `builder` =
`core/src/dom_builder.rs` (the module `azul_core::dom::builder`, declared at the end of `core/src/dom.rs`).

**State in one paragraph.** `DomBuilder` exists (core, api.json), builds a UI tree into shared arenas with nothing
allocated per node, makes each node's two fingerprint tiers as the node is closed, and hands the result back as a
`Dom` that carries the tree flat. Everything that needs a tree gets one (`Dom::expand_flat`, called by every builder
method that takes children and by every way to a `StyledDom`); `fingerprint_built_dom` reads the fingerprints of
either form; `StyledDom` styles a flat `Dom` from its arenas. A VIEW's rebuild reads the flat form today (a view that
built what it built before is kept without its DOM ever becoming a tree). The WINDOW's rebuild expands it first and
gains nothing until two lines of `regenerate_layout` change (`## For area diff-first`: that function is claimed by
area diff-first). One widget (the waveform) builds through it under `AZ_FLAT_WIDGETS=1`.

## Claims

Functions and types this task wrote or restructured:

- `core/src/dom_builder.rs` (new): `DomBuilder`, `FlatDom`, `fingerprint_built_dom`, `fingerprint_built_dom_against`,
  `flat_widgets_enabled`, `set_flat_widgets_on_this_thread`, and on `Dom`: `is_flat`, `flat`, `expand_flat`,
  `flat_node_data_at`, `flat_holds_node_type`, `into_fast_dom`.
- `core/src/dom.rs`: field `NodeDataExt::flat` (with `cmp_key`, `is_identity_empty`); the first lines of
  `Dom::add_child`, `Dom::set_children`, `Dom::fixup_children_estimated`, `FromIterator for Dom`; the module
  declaration and re-exports at the end.
- `core/src/layout_inputs.rs`: row `NodeField::Flat` (derived; hashes, carries and compares nothing).
- `core/src/styled_dom.rs`: `StyledDom::create_from_fast_dom` is a wrapper of the new
  `create_from_fast_dom_inheriting`; the first lines of `create_from_dom_inheriting` (a flat `Dom` goes to the arenas).
- `core/src/diff.rs`: one word (`hash_inline_style` is `pub(crate)`). `fingerprint_dom` is NOT touched (diff-first's).
- `layout/src/window.rs`: first lines of `style_user_dom_inheriting` and `resolve_form_controls`; new
  `resolve_form_controls_keeping_flat`; one call in the view's render (`fingerprint_built_dom`).
- `dll/src/desktop/shell2/common/transient.rs`: `looks_different` (one line).
- `layout/src/widgets/waveform.rs`: `build` (now `build_tree` / `build_flat`).
- `api.json`: class `dom.DomBuilder`.
- Tests: `core/src/dom_builder_test.rs` (new), one test in `layout/src/widgets/waveform.rs`, one list entry in
  `dll/src/desktop/shell2/headless/tests/every_holder_of_a_nodes_data_is_repointed.rs`.

Not touched (diff-first's): `regenerate_layout`, `fingerprint_dom`, `same_tree_changes`,
`changed_nodes_are_adoptable`, `adopt_changed_nodes`, `begin_reconciliation`, `reconcile_dom`, the cascade.

## The measurement: what building one node costs (by reading)

Units as in `audit/A3-window.md`: an allocation 20-50 ns (a free about the same), a SipHash of a small node
60-150 ns. ESTIMATES, not measurements. Sizes: `NodeData` is a `NodeType` (16 B payload + tag), a handler vector and
the two vectors of its `Css` (each `ptr, len, cap, destructor, flags` = 48 B), flags and three boxes: about 190 B; a
`Dom` adds two vectors and a count: about 290 B (A7-editor's number for `AzDom`).

**What a `Dom` node costs today, call by call** (`core/src/dom.rs`):
- a text (`NodeData::create_text..` `:3185`): the string (1, none when the app hands over a `String`) and
  `BoxOrStatic::heap` (1). The normalization (`normalize_input_az_string`) allocates only when a character changes.
- the first class, id, key, dataset or attribute: `Box<NodeDataExt>` (1; `:1904`, about 250 B zeroed).
- `add_class` / `add_id` (`:4035-4049`): the list is swapped out, pushed (1 for the first; capacity 4) and handed to
  `set_attributes` (`:3128`), which maps every attribute through `with_ruled_text` and collects again (in place when
  the standard library reuses the buffer, else 1 more and a free): each added class re-reads all earlier ones. The
  class string: 1 unless it is a `const` string. `Dom::with_attribute` (`:7839`) CLONES the whole list first.
- `add_css_property` (`:4056`): a `CssRuleBlock` with a declaration vector of one (1 per property) pushed onto the
  rule vector (1 for the first; a rule is 112 B, capacity 4, then doubling). `with_css_props`: the same rules
  (`css/src/css.rs:365`) plus the vector the caller built.
- `add_callback` (`:4021`): the handler vector (1 for the first; capacity 4).
- `add_child` (`:7559`): the parent's `Vec<Dom>` (1 for the first child; doubling moves 290 B per child each time;
  12,500 children under one parent: 14 growths, 7 MB moved).
- through the C API (`AzDom_with*`): each `with_*` copies the 290 B `Dom` in and out (A7-editor-03); the in-place
  `add_*` / `set_*` pass a pointer; `Dom::create_*` returns 290 B by value; a `Vec<Dom>::push` moves 290 B.
- then, per rebuild: `fingerprint_dom` (`core/src/diff.rs:3386`): a structure hash (19 rows of the node table) and
  a style hash per node, a third hash per text (its shape), a `NodeData` with a clone of the handler vector per
  node with handlers (1 allocation); on the full path three walks and the flatten
  (`styled_dom.rs` `convert_dom_into_compact_dom`: n default nodes written first, then overwritten); on every exit
  that keeps the styled DOM the tree is dropped: one free per allocation above.

**The three shapes** (allocations are per shape; times per shape, build + fingerprint + drop):

| shape | `Dom` today: allocations | `Dom` today: est. time | `DomBuilder`: allocations | `DomBuilder`: est. time |
|---|---|---|---|---|
| a plain text node | 2 (string, box) + its share of the parent's child list | 50-100 + 3 hashes 180-450 + 40-100 = 270-650 ns | 0 | pushes 10-20 + 3 hashes 180-450 = 190-470 ns |
| a styled `div`, two classes, one inline property | 4 with `const` class names (ext box, attribute list, rule list, declaration list), 6 with names from `&str` | 100-300 + 2 hashes 120-300 + 80-300 = 300-900 ns | 0 | pushes 20-40 + 2 hashes 120-300 = 140-340 ns |
| a label of the editor: a keyed `span` with a class, `left` / `top` (/ `transform`), three handlers, and its text (2 nodes) | 11 (span: ext box, class string, attribute list, rule list, 2 declaration lists, handler list, child list; text: 2; +1 in `set_attributes` when the collect is not in place), 13 when turned (a third declaration list and the transform's own function list); + the app's handle `RefAny` (2) | 300-650 + 5 hashes 300-750 + carried handlers 60-100 + 220-650 = 880-2,150 ns | 0 (the transform's function list and the handle `RefAny` are the app's values and stay: 1 + 2) | pushes 50-90 + 5 hashes 300-750 + carried handlers 60-100 + 10-20 = 420-960 ns |

What the engine does to a `Dom` before any diff, and what is left of it for a builder-made one:
- form controls (`resolve_form_controls`: one walk): a scan of the node kinds (1-2 ns per node) when the DOM stays
  flat (`resolve_form_controls_keeping_flat`).
- `fingerprint_dom` (the walk and 2-3 hashes per node): the hashes are made as the nodes close; asking for the
  fingerprints hashes ONE node (the root, which the `Dom` holds and may have changed), copies the tiers (28 B per
  node) and folds them (two u64 per node into two hashers).
- the carried values (`PreCascadeTransfers`: a `NodeData` per node with a handler, dataset, view, menu...): made when
  the fingerprints are asked for, one allocation per such node, as today. Not removed: the type is diff-first's.
- the drop: about a dozen frees for the arenas plus one reference-count decrement per handler, instead of one free
  per allocation.

## What an identical rebuild of 4,000 builder-made nodes costs (estimate)

2,000 labels (span + text), three handlers per span; stage numbers of `audit/A3-window.md`.

| stage | `Dom` today | builder-made |
|---|---|---|
| 2 the app builds it | 2,000 x 300-650 = 0.6-1.3 ms (the trace's 1.2-4 for typed properties) | pushes 0.1-0.2 ms + the tiers 2,000 x 300-750 = 0.6-1.5 ms |
| 3 form controls | 0.03 ms | 0.01 ms (a scan of kinds) |
| 4 `fingerprint_dom` | 0.6-1.6 ms | 0.05-0.08 ms (copy 112 KB, fold 8,000 u64, one node hashed) |
| 4b carried values | in stage 4 (0.12-0.2 ms) | 2,000 x 60-100 = 0.12-0.2 ms |
| 6A fresh handlers onto the kept DOM | 0.1-0.3 ms | the same |
| 7A runtime states | 0.08 ms | the same |
| 9 drop | 0.4-1.2 ms | 0.03-0.06 ms |
| sum | 2.5-7 ms (the trace) | 1.1-2.4 ms; 0.9-2.0 without handlers |

What is still above the user's 1-2 ms at the upper end, in order:
1. THE HASHES (0.6-1.5 ms): two SipHash-1-3 runs per node (three per text) over every row of the node table.
   `crate::hash::DefaultHasher` is `std`'s SipHash; core has `hash::FastHasher` (Fx-style, 3-5 times cheaper on small
   inputs). The tiers are compared within one process only, so nothing needs SipHash: switching `fingerprint_dom`
   and the builder TOGETHER (both name `crate::hash::DefaultHasher`; the equality test of this task holds them to each
   other) would bring this stage to 0.15-0.4 ms. Not done: `fingerprint_dom` is diff-first's.
2. the carried values and `adopt_fresh_payloads` (0.2-0.5 ms with a handler on every second node): a `NodeData` per
   node only to move a handler list. A borrowed form (the kept node takes the handlers straight out of the flat
   arena: `FlatDom` holds them in one vector, in node order) removes the allocation. Not done: `PreCascadeTransfers`
   and `adopt_fresh_payloads` are A3-window's / diff-first's.
3. the app's own values per node (the handle `RefAny`: 2 allocations per label; a transform's function list).
4. not building what did not change at all: the "handle" of diff-first-09 (a component hands back the subtree hash
   it got last time). The builder is where it would go (`open_unchanged(subtree_hash, len)`); it needs the early
   diff to accept "equal subtree" without the nodes, which is diff-first-04's map. Design only.

## What `Dom` can express, checked against the builder

| `Dom` / `NodeData` | builder | form |
|---|---|---|
| a node of any `NodeType` (`Dom::create_node`, the 190 `create_*`) | `open(NodeType)`, `add_node` | compact |
| a text (`create_text`: normalized; a `tr` key) | `open_text` / `add_text` (`_str`: no allocation); a `tr` key is held whole | compact / whole |
| children (`with_child`, `add_child`, `with_children`, `collect`) | the nodes between `open` and `close` | hierarchy array |
| a subtree built as a `Dom` (a widget) | `add_dom(Dom)` (each node held whole) | whole |
| class, id (`add_class`, `with_id`, `with_ids_and_classes`) | `add_class`, `add_id` (`_str`) | shared string |
| attributes (`with_attribute`, `set_attributes`) | `add_attribute` (ruled as `set_attributes` rules them) | arena |
| inline property with or without conditions (`add_css_property`, `with_css_props`) | `add_css_property` | arena |
| an inline `Css` (`with_style`), a parsed inline string (`NodeData::set_css`) | `node_data_mut().set_style(..)` / `.set_css(..)` | whole |
| handlers (`add_callback`, `with_callbacks`) | `add_callback` | arena |
| dataset (`set_dataset`) | `set_dataset` | arena |
| key (`set_key`) | `set_key` (the same hash of the same value) | inline |
| tab index, contenteditable | `set_tab_index`, `set_contenteditable` | inline |
| a sheet for the subtree (`Dom::with_css(&str)`, `add_component_css`) | `set_css(&str)`, `add_component_css` | by owner |
| accessibility, menus, SVG data / clip mask, marker, merge callback, component origin, animation callbacks, Fluent arguments, part-of-parent | `node_data_mut()` (the open node becomes a whole `NodeData`) or `open_node(NodeData)` | whole |
| a `VirtualView` with callback, data and version | `open_node(NodeData::create_virtual_view(..).with_virtual_view_version(v))` | whole |
| an image | `open(NodeType::Image(..))` | compact (the type by value) |

`set_children` (replace) and reading back (`Dom` is create-only) have no counterpart. A "whole" node costs what it
costs in a `Dom`; the nodes around it stay compact.

## The design as built

- **`FlatDom`** (`builder`): `nodes: Vec<FlatNode>` (the kind - a `NodeType` by value, a run of the shared string for
  a text, or an index into `full` - three runs into the arenas, flags, key, dataset index: about 80 B),
  `hierarchy: Vec<NodeHierarchyItem>` (written at `open`: parent, previous sibling, the previous sibling's next, the
  parent's last child), `text: String`, `attrs`, `props`, `callbacks`, `datasets`, `full: Vec<NodeData>`,
  `sheets: Vec<(node, Css)>`, the `DomFingerprints` vectors, the root's child count. A node's entries in an arena
  are consecutive because only the node pushed last takes any (a setter after a child is refused: `debug_assert`).
- **The tiers at `close`** (`FlatDom::hash_node`): for a whole node the functions of `fingerprint_dom` over its
  `NodeData` (copied as `full_structure_hash`, `full_text_shape_hash`, `full_style_hash`: its nested functions are
  private to it). For a compact node `compact_shape_hash` walks `NodeField::ALL` in order: the rows whose values
  sit in the arenas (`Callbacks`, `Attributes`, `Key`) are hashed from the arenas exactly as `hash_shape` hashes
  them from a node (`AzString`'s hash is its `str`'s); every other row is `NodeField::hash_shape` itself over a
  `NodeData` that holds the node's type and flags and nothing else (a compact node has no other value). The type
  MOVES into that node and back; a shared text is seen through a borrowed `AzString` / `BoxOrStatic::Static` view
  (the one `unsafe` lifetime extension, local to the block; nothing it is handed to keeps a copy). The style tier:
  per property its conditions, then the property (`hash_inline_style` over one rule per property), then the sheets.
- **The carrying `Dom`** (`DomBuilder::finish` -> `FlatDom::into_dom`): `root` = node 0 as a `NodeData`, `css` = the
  root's sheets, `children` empty, `estimated_total_children` = n - 1, and `root.extra.flat = Some(Box<FlatDom>)`.
  The root is single-sourced in the `Dom`: whatever is written onto the carrying `Dom`'s root or `css` afterwards is
  what every consumer reads (`fingerprints_with_root` hashes the root from the `Dom`; `expand_flat` and
  `into_fast_dom` take it from there). `NodeDataExt` compares and hashes `flat` (by the tiers below the root), so two
  carried trees are not equal because their roots are.
- **Where a tree is needed**: `Dom::expand_flat` (drains the arenas into `Dom`s: each node moved, not cloned). Called
  by `add_child` (both sides), `set_children` (the children; the carried ones are the ones replaced), `collect`,
  `fixup_children_estimated` (so `StyledDom::create*`, `convert_dom_into_compact_dom`, at every node), and in layout
  by `style_user_dom_inheriting` and `resolve_form_controls`.
- **Where the flat form is read**: `fingerprint_built_dom(&Dom)`; `Dom::into_fast_dom` +
  `StyledDom::create_from_fast_dom_inheriting` (sheets in `take_css_from_dom`'s order: sorted by where the owner's
  subtree ends, inner first); `Dom::flat_node_data_at(i)` (one node copied out: for the changed nodes of a rebuild);
  `Dom::flat_holds_node_type`.
- **The hole that is left**: a carrying `Dom` written DIRECTLY into another `Dom`'s `children` field (not through
  `add_child` / `set_children` / `collect`) is expanded by every way to a `StyledDom`, but `fingerprint_dom` (a
  `&Dom` walk) reads it as a leaf. `finish`'s documentation says so. The complete fix is in `fingerprint_dom`'s
  walk: `if let Some(flat) = dom.flat() { append flat's tiers at the current offset }` (For area diff-first, 3).

## Parts

### dom-builder-01 the builder, the carrying `Dom`, the entry points
- file:line: `core/src/dom_builder.rs`; `core/src/dom.rs` (`NodeDataExt`, `add_child`, `set_children`,
  `fixup_children_estimated`, `FromIterator`); `core/src/styled_dom.rs` (`create_from_fast_dom_inheriting`,
  `create_from_dom_inheriting`); `layout/src/window.rs` (`style_user_dom_inheriting`, `resolve_form_controls`, the
  view's render)
- class: waste, per rebuild: the floor of every exit (stages 2, 4, 9)
- test: core `dom_builder_test::audit_dom_builder_tests`: `a_ui_built_both_ways_is_the_same_tree_with_the_same_fingerprints`,
  `a_ui_built_both_ways_is_the_same_styled_dom`, `a_rebuild_from_one_form_to_the_other_is_unchanged_to_the_early_diff`,
  `a_flat_dom_becomes_its_tree_wherever_a_tree_is_needed`,
  `building_labels_through_the_builder_allocates_for_the_arenas_only`,
  `the_fingerprints_of_a_builder_made_tree_hash_its_root_only`
- severity: high
- status: fixed 10ec2ca92

### dom-builder-02 the root of a carrying `Dom` copied with its whole tree
- file:line: `core/src/dom_builder.rs` `Dom::flat_node_data_at`, `node_without_flat`; new
  `fingerprint_built_dom_against`
- class: wrong-output (found reading 01 again: `NodeData::clone` copies `extra.flat`)
- test: the first test of 01, at node 0
- severity: medium
- status: fixed 367346140

### dom-builder-03 api.json
- status: fixed 61011e00d (see `## api.json`)
- severity: medium

### dom-builder-04 one real user: the waveform widget
- file:line: `layout/src/widgets/waveform.rs` `build`, `build_flat`, `build_tree`
- why not the node graph: A3-solver rewrites `node_graph.rs` on its branch (814 lines changed against integration).
- test: `waveform_widget_tests::a_waveform_built_flat_is_the_waveform_built_as_a_tree`
- severity: low
- status: fixed dea65c348

### dom-builder-05 the window's rebuild does not read the flat form yet
- file:line: `layout.rs` `regenerate_layout` `:667`, `:691`
- class: waste: a builder-made window DOM is expanded to a tree before the early diff (correct, and slower than a
  `Dom` by the expansion) until the two calls change
- severity: high
- status: open: two lines in a function area diff-first claims; the exact change is under `## For area diff-first` (1)

### dom-builder-06 a changed rebuild of a flat DOM expands the whole tree
- file:line: `core/src/dom_builder.rs` `fingerprint_built_dom_against`; `core/src/diff.rs` `nodes_at` and the four
  functions on it
- class: waste: when anything differs from the build before, the flat `Dom` becomes its tree (n nodes materialized,
  about what building the `Dom` costs) although the exits for texts and for changed nodes read only the changed
  nodes
- fix (designed, not made: the functions are A3-window's / diff-first's): `nodes_at` answers `Vec<Cow<NodeData>>`
  (or the callers take `Dom::flat_node_data_at(i)` for a flat `Dom`): `fresh_texts_are_adoptable`,
  `adopt_fresh_texts`, `changed_nodes_are_adoptable`, `adopt_changed_nodes` then read k nodes; the full path alone
  expands (`style_user_dom_inheriting`), or styles from the arenas once its passes (form controls, webview fallback,
  translation, theme spin, icons) have a flat form or say "nothing to do" from a scan of the kinds
  (`flat_holds_node_type` is that scan).
- severity: medium
- status: open: needs `nodes_at` and its four users changed (diff-first's and A3-window's functions); the accessor `Dom::flat_node_data_at` they need exists and is tested

### Checked: fine
- `NodeField::ALL` has 20 rows now; the new row hashes nothing, so every existing fingerprint is the number it was.
- `with_ruled_text` leaves ids and classes alone and is idempotent for the rest (`normalize_input_az_string` returns
  its input when no character changes): ruling an attribute once (builder) equals ruling it at every later
  `add_class` (`Dom`).
- `Css::from(CssPropertyWithConditionsVec)` (`css/src/css.rs:365`) and `add_css_property` make the same rule per
  property: a `with_css_props` node and a builder node with the same properties are equal.
- `hierarchy.subtree_len` counts descendants (`styled_dom.rs:912`), so `create_from_fast_dom`'s scope
  `[owner, owner + subtree_len]` is the tree path's `[start, start + estimated_total_children]`.
- `clone_self` of a `NoDestructor` vector copies the pointer: the borrowed text view is never cloned (the hash
  functions it is handed to were read: `hash_node_type_shape`, `NodeField::hash_shape`), and the carried values are
  built from owned copies, never from a view.

## api.json

One ADDITION, nothing changed or removed: module `dom`, class `DomBuilder`
(`external: azul_core::dom::DomBuilder`, `is_boxed_object`, fields `ptr: *mut c_void`, `run_destructor: bool`;
`custom_impls: Clone, Debug, Default, Drop`).
- constructors: `create()`, `with_capacity(nodes: usize)`.
- functions (`self: refmut` unless noted): `open(node_type: NodeType)`, `open_node(node: NodeData)`,
  `open_text(text: String)`, `close()`, `add_node(node_type: NodeType)`, `add_text(text: String)`, `add_dom(dom: Dom)`,
  `add_class(class: String)`, `add_id(id: String)`, `add_attribute(attr: AttributeType)`,
  `add_css_property(prop: CssPropertyWithConditions)`, `add_callback(event: EventFilter, data: RefAny, callback:
  CallbackType)`, `set_key(key: String)`, `set_tab_index(tab_index: TabIndex)`, `set_contenteditable(bool)`,
  `set_dataset(data: OptionRefAny)`, `add_component_css(css: Css)`, `set_css(style: String)`,
  `node_count(self: ref) -> usize`, `finish(self: value) -> Dom`.
- The generators have to be run for the class (the dll's `AzDomBuilder_*` exports, `azul::dom::DomBuilder` of the
  Rust API, every binding). `add_callback`'s body is `Dom::add_callback`'s
  (`azul_layout::callbacks::Callback::create(callback).to_core()`); `set_css` needs core's `css_parser` feature, as
  `Dom::set_css` does.
- Not in api.json (Rust only): `add_class_str`, `add_id_str`, `add_text_str`, `open_text_str` (a string slice: no
  `AzString` is made), `set_key<K: Hash>`, `node_data_mut`, and `Dom::is_flat` / `expand_flat` / the flat readers.
  If the editor's label loop is to pay nothing per text, `Refstr` variants of `add_text` / `add_class` are the next
  addition (`String` by value costs the app one allocation per call unless it holds a `const` string).

## Switches

- `AZ_FLAT_WIDGETS` (read once per process, `azul_core::dom::flat_widgets_enabled`): unset, empty or `0` = widgets
  build trees (default); anything else = a widget that has both builds makes its DOM through `DomBuilder` (today:
  the waveform). Default OFF because code and tests outside the engine walk a widget's `dom.children`
  (`theme_checks::nodes`), which a flat `Dom` does not have before `expand_flat`. A test picks per thread:
  `azul_core::dom::set_flat_widgets_on_this_thread(Some(bool))`.
- No switch for the builder itself: an app opts in by calling it. A `Dom` built the old way takes the old path
  unchanged (one pointer test per `add_child` / per node of `fixup_children_estimated`).

## For area diff-first

1. **The two lines that make the window's rebuild read the flat form** (`layout.rs`, `regenerate_layout`):
   ```
   -    let _ = layout_window.resolve_form_controls(&mut user_dom);
   +    let _ = layout_window.resolve_form_controls_keeping_flat(&mut user_dom);
   ...
   -        Some(azul_core::diff::fingerprint_dom(&user_dom))
   +        Some(azul_core::dom::fingerprint_built_dom_against(
   +            &mut user_dom,
   +            layout_window.last_dom_fingerprints.as_ref(),
   +        ))
   ```
   Why nothing else in the function has to change: `fingerprint_built_dom_against` leaves the `Dom` flat only when
   its fingerprints EQUAL the build before's (the whole struct, so also the fold compare of exit A). Then
   `texts_alone_changed` and `same_tree_changes` answer `None`, no `nodes_at` runs, and the only later reader of
   `user_dom` is `style_user_dom_for` (when a gate of exit A fails: theme, CSD, node count), which expands it. In
   every other case the `Dom` is a tree before the function returns, and the code below reads it as today.
   `resolve_form_controls_keeping_flat` keeps the DOM flat unless a node's type is one the resolver's first walk looks
   for (`form`, `button`, `input`, `select`, `textarea`, `datalist`). On Linux `inject_software_menubar` reads
   `user_dom.root` (the real root) and wraps the DOM with `with_children`, which expands it: correct, not fast.
   Count test to add with it (dll, beside `an_identical_rebuild_and_a_changed_text_style_no_node`): a window whose
   layout callback builds through `DomBuilder`; an identical rebuild: `kept` exit, 0 nodes styled, and
   `azul_core::dom::builder::NODES_HASHED_AT_FINGERPRINT` (test builds of core only; expose a counter if the dll
   test needs one) = 1.
2. **Changed nodes without the tree** (dom-builder-06): `nodes_at(fresh, changed)` for a flat `Dom` =
   `changed.iter().filter_map(|i| fresh.flat_node_data_at(*i))` (owned nodes; the root is index 0), in
   `fresh_texts_are_adoptable`, `adopt_fresh_texts`, `changed_nodes_are_adoptable`, `adopt_changed_nodes`; then
   `fingerprint_built_dom` (the `&Dom` one, no expansion) replaces `.._against` at the call above and the full path
   alone expands.
3. **`fingerprint_dom` itself**: a first line `if dom.is_flat() { return crate::dom::fingerprint_built_dom(dom); }`
   and, in `walk`, the same test for a child (append the child's tiers at the current offset: `subtree_len` and the
   `(index, hash)` lists shifted by it) close the hole of a carrying `Dom` written straight into a `children` field,
   and make every caller of `fingerprint_dom` flat-aware at once. The builder's per-node functions
   (`full_structure_hash`, `full_text_shape_hash`, `full_style_hash`, `hash_sheets` in `builder`) are copies of
   `fingerprint_dom`'s nested ones: if those move to module level, the builder should call them
   (`a_ui_built_both_ways_is_the_same_tree_with_the_same_fingerprints` fails on any drift until then).
4. **The hasher** (see "what is still above", 1): `crate::hash::FastHasher` for both tiers, in `fingerprint_dom` and
   in `builder` (five `DefaultHasher::new()` there) in ONE commit.
5. **A view's changed rebuild** (`reconcile_rematerialized_child`, your "Next (1)"): the view's site already calls
   `fingerprint_built_dom(&dom)`; the flat `Dom` is expanded by `style_user_dom_inheriting`. The three exits there can
   read `dom.flat_node_data_at(i)` as in (2).

## For area A7-editor (m4p)

Text only; the editor's repository is not this task's. After the generators ran for `DomBuilder` and the editor's
azul pin moved:

`mapedit3/src/ui/main/map.rs`, `place_labels` / `render_texts` / `render_symbols` / `picture` (the label loop of
`render_map_view`): build the picture's children into ONE `DomBuilder` instead of a `Vec<Dom>`.

```rust
// picture(): the root first (it was built last, around `children`)
let mut b = DomBuilder::with_capacity(expected_nodes);          // 2 per label piece + tiles + symbols + 1
b.open_node(root_node_data);                                    // marker, transform / origin / user-select as today
if !sheet.text.is_empty() { b.add_component_css(Css::from_string(sheet.text.clone())); }

// place_labels(), per piece (was: Dom::create_span_with_text + set_key + place + add_item_callbacks + out.add)
b.open(NodeType::Span);
b.set_key(p.key.clone());                                       // the same key: AzString hashes as its str
b.add_class(class.clone());                                     // hold the layer's class as an AzString
b.add_css_property(CssPropertyWithConditions::simple(CssProperty::left(..)));
b.add_css_property(CssPropertyWithConditions::simple(CssProperty::top(..)));
if p.angle != 0.0 { b.add_css_property(.. transform ..); }
b.add_callback(EventFilter::Hover(HoverEventFilter::LeftMouseDown), handle.clone(), on_item_mouse_down);
b.add_callback(EventFilter::Hover(HoverEventFilter::MouseEnter), handle.clone(), on_item_mouse_enter);
b.add_callback(EventFilter::Hover(HoverEventFilter::MouseLeave), handle, on_item_mouse_leave);
b.add_text(p.text.clone());                                     // the text node, as create_span_with_text made it
b.close();

// everything that stays a Dom (tiles, symbols, the selected piece with `own_css` and a marker, the cleanup marks):
b.add_dom(dom);                                                 // held whole, in its place among the children

VirtualViewReturn::with_dom(b.finish(), whole, whole)
```
- The setters come BEFORE the text child (the builder takes a node's own values before its first child); the
  resulting nodes, order, classes, properties and handlers are the ones the `Dom` calls make, and the fingerprints
  are equal: a render that switches from one form to the other is "unchanged" to the view's early diff.
- `Grouped` (groups of n under a `div`): `b.open(NodeType::Div)` ... `b.close()` around each group; with
  `LABEL_GROUP = 0` nothing.
- The selected piece (`own_css` + `set_marker`): `b.set_css(..)` is `Dom::with_css` (a sheet for the node's
  subtree); the marker needs `Dom` / `NodeData` (`add_dom` of the piece built as today): one node per render.
- What it buys, per piece: the 11 allocations A7-editor-03 counted as "azul's node construction" go (the text,
  the child list, the class string, the attribute box and list, the rule list and declaration lists, the handler
  list); the one FFI copy of the span and the move into the group go (every builder call passes one pointer);
  `p.text.clone()` and `class.clone()` remain as the app's own until `Refstr` variants exist (`## api.json`).
  Left per piece: the handle `RefAny` (2) and a turned label's transform list (1).
- And per render that builds what the last one built (a `trigger_virtual_view_rerender` after a pan that moved no
  label in or out, a hover): azul's `fingerprint_dom` walk of 12.5k nodes and the drop of the tree go; the view is
  reclaimed from the builder's tiers. This path is wired (`layout/src/window.rs`, the view's render).
- The test `a_render_allocates_a_bounded_number_of_times_per_label_piece` (at most 16 per piece) can tighten to
  the app's own: at most 5 per piece (handle 2, text clone 1, class clone 1, transform 1).

## What to look at on the real window

Nothing changes for an app that does not call `DomBuilder` (the editor today). With `AZ_FLAT_WIDGETS=1`: a
waveform (media controls) looks and seeks as without it.

## Not read

- `core/src/dom.rs` 262-1370 (`NodeType`'s variants and `On`) beyond the variants named here; the 190 `Dom::create_*`
  constructors (4952-7500) beyond `create_node`, `create_from_data`, `create_text..`, `create_span_with_text`: they
  are `create_node` + `NodeData` setters by their signatures.
- `core/src/styled_dom.rs` `create_from_compact_dom` below its first 80 lines (the cascade: diff-first's).
- `core/src/xml.rs` `CompactDomBuilder` (the other producer of `FastDom`): untouched, its `FastDom` literal still
  compiles (no field was added to `FastDom`).
- `layout.rs` `regenerate_layout` beyond the lines that name `user_dom` (grep) and 580-760.
- `layout/src/form_controls.rs` beyond `resolve_form_controls_in_dom` and `prepass`.
- The generators (`doc/`, `dll/build`): how `is_boxed_object` and `custom_impls` are turned into bindings was
  inferred from `SharedBytes`, `CssPropertyCachePtr` and `Dom`'s entries, not read.

## Hand-over (kept current at every commit)

**Built and committed** (all `[unverified]`: nothing was compiled):
- `10ec2ca92` dom-builder-01: `core/src/dom_builder.rs` (the builder, `FlatDom`, the carrying `Dom`, the readers),
  the hooks in `core/src/dom.rs`, the row `NodeField::Flat`, `StyledDom::create_from_fast_dom_inheriting`, the layout
  and dll call sites, and the six tests of `core/src/dom_builder_test.rs`.
- `367346140` dom-builder-02: `node_without_flat`, `fingerprint_built_dom_against`.
- `61011e00d` dom-builder-03: api.json `dom.DomBuilder`.
- `dea65c348` dom-builder-04: `flat_widgets_enabled` (`AZ_FLAT_WIDGETS`), the waveform's `build_flat`, its test.

**Tests to run** (core first; each names what it guards in its doc comment):
- `cargo test -p azul-core --lib audit_dom_builder_tests` (six tests; this test binary now has a counting
  `#[global_allocator]`, declared in `core/src/dom_builder_test.rs` under `cfg(all(test, feature = "std"))`: if
  another one is ever added to core's tests, keep one).
- `cargo test -p azul-core --lib every_field_of_a_node_is_a_row_of_the_table` (the new field `flat` has its row).
- `cargo test -p azul-layout --lib a_waveform_built_flat_is_the_waveform_built_as_a_tree` and the three older
  `waveform_widget_tests` (default path unchanged).
- dll: `every_holder_of_a_nodes_data_is_reached_by_the_repointing_walk` (the new file is listed as `BEING_BUILT`).
- `cargo test -p azul-core --doc DomBuilder` (the example on the type).

**Where a compile error is most likely** (read these first when the verifier reports one):
1. `builder::FlatDom::hash_node`: three arms that borrow `self.nodes` mutably and then call `&self` methods (NLL
   has to end the borrow at the last use of the arm's binding). If it does not: copy `flags` and the kind's payload
   out before the `match` (the `Element` arm already moves the type out with `mem::replace`).
2. `NodeDataExt::cmp_key` is now a tuple of 12 (the largest arity `PartialEq` / `Ord` / `Hash` are implemented
   for). One more field: split it into two nested tuples.
3. `builder::node_without_flat` names every field of `NodeDataExt` and of `NodeData`: a field another area adds
   shows up there as a missing-field error (add it to the literal).
4. `core/src/dom_builder_test.rs`: paths of the css types (`azul_css::props::basic::pixel::PixelValue`,
   `props::layout::position::{LayoutLeft, LayoutTop}`), `AttributeType::Data` / `Hidden` / `Title`.
5. api.json: `DomBuilder` was written after `SharedBytes` / `CssPropertyCachePtr` / `Dom`; the generators were not
   read. `set_css` compiles only with core's `css_parser` feature (as `Dom::set_css`).

**Generator steps** (for whoever runs codegen): the api.json addition is one class in module `dom`
(`## api.json`). Run the normal api.json -> dll / bindings generation; nothing existing changed, so no binding
breaks. Until it ran, the builder is reachable from Rust inside the engine (`azul_core::dom::DomBuilder`) and from
widgets, not from an app that uses `azul::...` (the editor).

**In flight**: nothing uncommitted.

**Left, with its design**:
- dom-builder-05 (two lines in `regenerate_layout`, diff-first's): `## For area diff-first` (1). Until then a
  builder-made WINDOW DOM is correct and not faster; a builder-made VIEW DOM is faster already.
- dom-builder-06 (changed nodes without the tree): `## For area diff-first` (2).
- The hasher and the carried values: "what is still above the user's 1-2 ms" (1), (2).
- The editor's label loop: `## For area A7-editor (m4p)` (exact code), after codegen and the pin.
- `Refstr` variants of `add_text` / `add_class` / `add_id` in api.json (`## api.json`, last bullet).
- More widgets behind `AZ_FLAT_WIDGETS` (the list view's rows, the node graph's node list once A3-solver's
  branch is merged): mirror `waveform::build_flat` (root whole, children through the setters, a test that compares
  fingerprints, `reads_alike` per node and the computed styles).
