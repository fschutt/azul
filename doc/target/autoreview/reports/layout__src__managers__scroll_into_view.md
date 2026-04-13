# Review: layout/src/managers/scroll_into_view.rs

## Summary
- Lines: 515
- Public functions: 3 (`scroll_node_into_view`, `scroll_cursor_into_view`, `calculate_axis_delta`)
- Crate-internal functions: 1 (`scroll_rect_into_view`)
- Public structs/enums: 1 (`ScrollAdjustment`)
- Re-exports: 3 (`ScrollIntoViewBehavior`, `ScrollIntoViewOptions`, `ScrollLogicalPosition`)
- Findings: 0 high, 2 medium, 1 low

## Findings

### [MEDIUM] Vibe-Coding Hint — TODO for CSS scroll-behavior
- **Location**: `scroll_into_view.rs:449`
- **Details**: `resolve_scroll_behavior` contains a TODO:
  ```rust
  // TODO: Check CSS scroll-behavior property on the scroll container
  // For now, default to instant
  ScrollIntoViewBehavior::Instant
  ```
  The function accepts `_dom_id`, `_node_id`, and `_layout_results` parameters that are all prefixed with `_` (unused). This is a partial implementation — the function signature is ready but the CSS property lookup is not wired up.
- **Recommendation**: Either implement the CSS `scroll-behavior` property lookup or document this as a known limitation. The unused parameters should retain their `_` prefix until implemented.

### [MEDIUM] Known Bug Pattern — Return values silently ignored at call sites
- **Location**: Callers in `dll/src/desktop/shell2/common/event.rs:1209,1446,2525,2644`
- **Details**: All four call sites of `scroll_node_into_view` discard the `Vec<ScrollAdjustment>` return value. While the function applies scroll adjustments as a side effect (so the scroll does happen), the discarded return value suggests the `ScrollAdjustment` type may not be providing value. Alternatively, callers may need the adjustments for animation coordination.
- **Recommendation**: If the return value is purely informational and no caller uses it, consider changing the return type to `()` or adding `#[must_use]` to force callers to handle it. If callers should use it (e.g., to trigger re-renders), that's a bug.

### [LOW] `Instant::clone()` unnecessary
- **Location**: `scroll_into_view.rs:144`
- **Details**: `now.clone()` is called on `Instant` which likely implements `Copy`. If so, `.clone()` is unnecessary and Clippy would flag it.
- **Recommendation**: Remove `.clone()` if `Instant` is `Copy`.

## System Documentation
- System identified: yes — Scrolling / Scroll Management system (part of the layout engine's event/interaction layer)
- Existing doc: none (no `scrolling.md` or similar in `doc/guide/`)
- Doc needed: A `doc/guide/scrolling.md` covering the scroll system architecture — `ScrollManager` (scroll state tracking, smooth scrolling, clamping), `scroll_into_view` (W3C CSSOM View Module), scroll hit-testing, scrollbar rendering, and virtual scrolling. Related files: `layout/src/managers/scroll_state.rs`, `layout/src/managers/scroll_into_view.rs`, `layout/src/managers/virtual_view.rs`.
