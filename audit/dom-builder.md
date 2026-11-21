# dom-builder: a `Dom` builder that produces the flat, single-allocation form

Task of the 2026-11-21 audit (RULES.md 8.8; `audit/diff-first.md`, block diff-first-09). Blind: nothing here was
compiled or run. Branch `audit/dom-builder`, cut from the tip of `audit/diff-first` (`cfc4d6d72`), `audit/integration`
merged at the start (`a03aed60c`).

## Claims

Functions and types this task writes or restructures (kept current):

- `core/src/dom.rs`: `FastDom` (new fields for the fingerprints the builder made), new `DomBuilder` and what it needs
  (`DomBuilderNode` handles, the conversion `Dom -> FastDom`).
- `core/src/styled_dom.rs`: `StyledDom::create_from_fast_dom` (and its inheriting variant), `convert_dom_into_compact_dom`
  only as a caller.
- `core/src/diff.rs`: NOT `fingerprint_dom`'s body (diff-first's). New functions beside it only: the per-node hash
  as a function over `&NodeData` that both `fingerprint_dom` and the builder call (a refactor of the loop body into a
  function, no change of what is hashed), and `fingerprint_fast_dom`.
- `core/src/callbacks.rs`: the layout callback's return (an addition beside `Dom`).
- `api.json`: class `DomBuilder` (addition).
- One widget of `layout/src/widgets/` (named in the block of part 6 when chosen).
- Tests: `core/src/dom_test.rs` (`mod audit_dom_builder_tests` at the end), `core/src/styled_dom_test.rs`
  (the same, after diff-first's module).

Not touched (diff-first's): `regenerate_layout`, `begin_reconciliation`, `reconcile_dom`, `recascade_nodes`, the
cascade. Where the shell has to accept the flat form: `## For area diff-first` below, or a few lines outside its
claimed functions.
