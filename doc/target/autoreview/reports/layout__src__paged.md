# Review: layout/src/paged.rs

## Summary
- Lines: 263
- Public functions: 11 (across `Fragmentainer`, `FragmentationContext`)
- Public structs/enums: 2 (`FragmentationContext`, `Fragmentainer`)
- Findings: 1 high, 1 medium, 2 low

## Findings

### [HIGH] Dead Code — `Fragmentainer` methods never called externally
- **Location**: `paged.rs:132-160` (`remaining_space`, `is_full`, `can_fit`, `use_space`)
- **Details**: All four `Fragmentainer` instance methods are never called from outside `paged.rs`. The `FragmentationContext` wrapper methods (`current()`, `current_mut()`, `advance()`) are used, but no code ever calls methods on the returned `Fragmentainer` references.
- **Evidence**: `grep "\.is_full\(\)|\.can_fit\(|\.use_space\(|\.remaining_space\("` in `layout/src/` returns only self-references within `paged.rs` itself (lines 145, 152). The `can_fit` hits in `fragmentation.rs:849,866` and `pagination.rs:183` are on different types (`FragmentationLayoutContext` and `PageGeometer`), not `Fragmentainer`.
- **Recommendation**: These methods should either be wired into the layout pipeline or removed. Currently they are dead code.

### [MEDIUM] Unwired methods — `fragmentainer_count`, `fragmentainers`, `page_size`
- **Location**: `paged.rs:188`, `paged.rs:270`, `paged.rs:280`
- **Details**: These `FragmentationContext` methods have no call sites outside `paged.rs`. Only `new_continuous`, `new_paged`, `page_content_height`, `is_paged`, `current`, `current_mut`, and `advance` are used externally.
- **Evidence**: Grep for `fragmentainer_count`, `\.fragmentainers\(\)`, `\.page_size\(\)` across layout/src returns only definitions in `paged.rs`.
- **Recommendation**: Mark as `#[allow(dead_code)]` with justification, or remove if not needed for the public API.

### [LOW] `#[allow(dead_code)]` on `MultiColumn` and `Regions` variants
- **Location**: `paged.rs:60`, `paged.rs:75`
- **Details**: The `MultiColumn` and `Regions` variants of `FragmentationContext` are marked `#[allow(dead_code)]` and have no call sites. They add match arms in every method but serve no current purpose.
- **Recommendation**: Acceptable if these are planned features, but the `#[allow(dead_code)]` should be documented with a brief comment explaining when these will be needed.

### [LOW] Magic number — `1.0` threshold in `is_full`
- **Location**: `paged.rs:145`
- **Details**: `self.remaining_space() < 1.0` uses `1.0` (pixel) as a "full" threshold. This is reasonable but could be a named constant for clarity.
- **Recommendation**: Consider `const FRAGMENTAINER_FULL_THRESHOLD: f32 = 1.0;`

## System Documentation
- System identified: yes — CSS Paged Media / Fragmentation system (part of the layout engine)
- Existing doc: none (no `doc/guide/` file for paged media, fragmentation, or the layout solver)
- Doc needed: A `doc/guide/paged-media.md` guide explaining the paged media architecture, how `paged.rs`, `fragmentation.rs`, `solver3/pagination.rs`, and `solver3/paged_layout.rs` relate to each other, and the overall pagination flow. The current split across four files with overlapping types is confusing without a guide.
