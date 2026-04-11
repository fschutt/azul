# Review: core/src/styled_dom.rs

## Summary
- Lines: ~2210
- Public functions: ~30 (methods + free functions)
- Public structs/enums: 13
- Findings: 0 high, 1 medium, 1 low

## Findings

### [MEDIUM] Stub/TODO Comment — `create_from_fast_dom`
- **Location**: `styled_dom.rs:858`
- **Details**: `//    (TODO: respect node_id scoping for sub-tree cascading)` — indicates that CSS scoping for per-node CSS is not yet implemented in the fast DOM path.
- **Recommendation**: Track this as a known limitation or implement node-scoped cascading.

### [LOW] Recursive function without depth limit — `recursive_get_last_child`
- **Location**: `styled_dom.rs:2517`
- **Details**: Recursive call with no depth limit. For extremely deep DOMs this could stack overflow. Also `convert_dom_into_compact_dom_internal` (line 2319) is recursive.
- **Recommendation**: Low risk in practice since DOM depth is typically bounded, but consider iterative versions for robustness.

## System Documentation
- System identified: yes — CSS Styling System (DOM styling, CSS cascade, restyle)
- Existing doc: `doc/guide/styling-system.md`, `doc/guide/css-styling.md`
- Doc needed: n/a (covered by existing guides)
