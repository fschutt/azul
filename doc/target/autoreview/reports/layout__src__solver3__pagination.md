# Review: layout/src/solver3/pagination.rs

## Summary
- Lines: 1224
- Public functions: 2 (`is_forced_break`, `is_avoid_break`, `calculate_pagination_offset`)
- Public structs/enums: 13 (`PageGeometer`, `PageMargins`, `BreakBehavior`, `BreakEvaluation`, `RepeatedTableHeader`, `PaginationContext`, `MarginBoxPosition`, `RunningElement`, `MarginBoxContent`, `CounterFormat`, `PageInfo`, `HeaderFooterConfig`, `PageTemplate`, `FakePageConfig`, `TableHeaderInfo`, `TableHeaderTracker`)
- Findings: 3 high, 2 medium, 1 low

## Findings

### [HIGH] Duplicated Functionality — Major overlap with `layout/src/fragmentation.rs`

- **Location**: Entire file vs `layout/src/fragmentation.rs`
- **Details**: `fragmentation.rs` contains parallel implementations of nearly every concept in this file:
  - `PageMargins` struct (pagination.rs:67 vs fragmentation.rs:488) — identical fields and methods
  - `PageTemplate` struct (pagination.rs:848 vs fragmentation.rs:210) — similar purpose, different shape
  - `CounterFormat` enum (pagination.rs:549) vs `PageNumberStyle` enum (fragmentation.rs:111) — same variants
  - `to_roman()` / `to_alpha()` / `to_greek()` (pagination.rs:582-658) vs `to_upper_roman()` / `to_lower_roman()` / `to_upper_alpha()` / `to_lower_alpha()` (fragmentation.rs:921-970) — duplicate roman numeral and alpha conversion logic
  - `MarginBoxContent` enum (pagination.rs:508) vs `PageSlotContent` enum (fragmentation.rs:149) — same concept
  - `MarginBoxPosition` enum (pagination.rs:433) vs `PageSlotPosition` enum (fragmentation.rs:132) — same concept (pagination has 16 positions, fragmentation has 6)
  - `PageInfo` (pagination.rs:663) vs `PageCounter` (fragmentation.rs:52) — overlapping page metadata
  - `BreakBehavior` (pagination.rs:203) vs `BoxBreakBehavior` (fragmentation.rs:339) — same break classification
  - `is_forced_break` / `is_avoid_break` — **FIXED**: fragmentation.rs now delegates to pagination.rs canonical implementations
- **Evidence**: Grep for `PageMargins` found both files; grep for `to_lower_roman` found fragmentation.rs with parallel implementations.
- **Recommendation**: Consolidate into a single module. The pagination.rs "infinite canvas" approach and fragmentation.rs's fragmentainer approach appear to be two competing designs for the same problem. Pick one and remove the other, or extract shared types (PageMargins, counter formatting, break behavior) into a common module.

### [HIGH] Dead Code — Many public types/functions have zero external callers

- **Location**: Multiple types and functions
- **Details**: The following public items are only referenced within `pagination.rs` itself (zero callers outside the module):
  - `PageGeometer` — grep found 0 files outside pagination.rs
  - `BreakEvaluation` — grep found 0 files outside pagination.rs
  - `BreakBehavior` — grep found 0 files outside pagination.rs (fragmentation.rs has its own `BoxBreakBehavior`)
  - `is_forced_break` — grep found 0 callers outside pagination.rs
  - `is_avoid_break` — grep found 0 callers outside pagination.rs (getters.rs has its own `is_avoid_break_inside`)
  - `calculate_pagination_offset` — grep found 0 callers outside pagination.rs
  - `PaginationContext` — grep found 0 callers outside pagination.rs
  - `RepeatedTableHeader` — grep found 0 callers outside pagination.rs
  - `MarginBoxPosition` — grep found 0 callers outside pagination.rs
  - `RunningElement` — grep found 0 callers outside pagination.rs
  - `CounterFormat` — grep found 0 callers outside pagination.rs
  - `PageTemplate` (the pagination.rs version) — grep found 0 callers outside pagination.rs (fragmentation.rs has its own `PageTemplate`)
- **Evidence**: Ran `Grep` for each symbol with `files_with_matches` mode. Only `pagination.rs` appeared for all items listed above.
- **Recommendation**: Either wire these types into the layout engine or remove them. The actually-used types are: `FakePageConfig` (used in paged_layout.rs and all test files), `HeaderFooterConfig` (display_list.rs), `MarginBoxContent` (display_list.rs), `PageInfo` (display_list.rs), `TableHeaderInfo` (display_list.rs), `TableHeaderTracker` (display_list.rs).

### [HIGH] Stub Code — TODO in generate_content for NamedString

- **Location**: `pagination.rs:809`
- **Details**: `generate_content()` has a TODO for named string lookup:
  ```rust
  MarginBoxContent::NamedString(name) => {
      // TODO: Look up named string from document context
      format!("[string:{}]", name)
  }
  ```
  This returns a placeholder `[string:name]` instead of actual content. Similarly, `RunningElement` returns `[element:name]` (line 813) which is also a placeholder.
- **Recommendation**: Either implement the lookup or document this as a known limitation. The placeholder output would be user-visible in rendered pages.

### [MEDIUM] Duplicated Default Impls — HeaderFooterConfig and FakePageConfig

- **Location**: `pagination.rs:721-740` vs `pagination.rs:956-980`
- **Details**: `HeaderFooterConfig::default()` and `FakePageConfig::default()` set nearly identical field values (same heights, font size, text color). `FakePageConfig` exists solely to produce a `HeaderFooterConfig` via `to_header_footer_config()`. The two structs have significant overlap.
- **Recommendation**: Consider whether `FakePageConfig` could be replaced by builder methods on `HeaderFooterConfig` directly, eliminating the intermediate type.

### [MEDIUM] Missing Documentation — `..Default::default()` usage with implicit field defaulting

- **Location**: `pagination.rs:753-754`, `pagination.rs:771`
- **Details**: `HeaderFooterConfig::with_page_numbers()` and `with_header_and_footer_page_numbers()` use `..Default::default()`. While verified safe (defaulted fields are `show_header: false`, `header_height: 30.0`, `font_size: 10.0`, `text_color: black`, `skip_first_page: false` — all reasonable), this pattern is fragile if new fields are added to `HeaderFooterConfig` later. No comment explains which fields are intentionally defaulted.
- **Recommendation**: Add a brief comment noting which fields are intentionally left at defaults, or use explicit field initialization.

## System Documentation
- System identified: yes — CSS Paged Media / Pagination / Layout Solver
- Existing doc: none (no `doc/guide/` file covers pagination or paged media)
- Doc needed: A guide covering the pagination/paged media system, explaining the "infinite canvas" approach, how `PageGeometer` and `FakePageConfig` integrate with the layout solver via `paged_layout.rs` and `display_list.rs`, and how it relates to (and overlaps with) `fragmentation.rs`. Should clarify which approach is canonical.
