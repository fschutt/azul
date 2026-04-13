# Review: layout/src/managers/a11y.rs

## Summary
- Lines: 805 (735 non-blank)
- Public functions: 4 (`new`, `update_tree`, `decode_a11y_node_id`, `map_accesskit_action`)
- Public structs/enums: 2 (`A11yManager`, `CursorA11yInfo`)
- Findings: 0 high, 2 medium, 1 low

## Findings

### [MEDIUM] Long function — `update_tree` is ~245 lines

- **Location**: `a11y.rs:79-334`
- **Details**: `update_tree` is a 245-line function with three distinct passes
  (node creation, parent-child wiring, children assignment) and inline text
  collection logic. The text/cursor handling block (lines 176-250) alone is ~75 lines.
- **Recommendation**: Extract helper functions:
  - `collect_text_content(...)` for lines 176-250
  - `build_parent_child_map(...)` for lines 256-294

### [MEDIUM] `build_node` and `node_type_to_role` are private but could be useful for testing

- **Location**: `a11y.rs:337` (`build_node`), `a11y.rs:507` (`node_type_to_role`), `a11y.rs:610` (`map_role`)
- **Details**: These are private helper functions with no external call sites.
  This is fine for encapsulation, but they are non-trivial mapping functions that
  would benefit from unit tests — especially `node_type_to_role` (100-line match)
  and `map_role` (70-line match).
- **Recommendation**: Add `#[cfg(test)]` unit tests for the role-mapping functions,
  or make them `pub(crate)` for integration testing.

### [LOW] Catch-all `_ => {}` in state/attribute matching

- **Location**: `a11y.rs:403`, `a11y.rs:462`
- **Details**: Both `AccessibilityState` and `AttributeType` match arms end with
  `_ => {}`. If new variants are added, these won't trigger a compiler warning.
- **Recommendation**: Consider whether exhaustive matching would be preferable to
  catch new variants at compile time, or add a comment explaining why the catch-all
  is intentional.

## System Documentation
- System identified: **Accessibility** (accesskit integration, screen reader support)
- Existing doc: none (no `doc/guide/accessibility.md` found)
- Doc needed: An accessibility guide covering:
  - How the a11y tree is built from the DOM (`A11yManager::update_tree`)
  - Incremental vs full tree updates
  - How action requests flow from screen readers back into Azul events
  - Platform adapter integration (macOS, Windows, X11, Wayland)
  - The `CursorA11yInfo` cursor/selection exposure mechanism
