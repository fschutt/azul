# Review: layout/src/fragmentation.rs

## Summary
- Lines: 972
- Public functions: 31 (methods + free fn `decide_break`)
- Public structs/enums: 15
- Findings: 2 high, 0 medium, 1 low

## Findings

### [HIGH] Stub Code — `generate_page_chrome` returns empty Vec
- **Location**: `fragmentation.rs:730-761`
- **Details**: The method iterates over slots, computes `_text` and `(_x, _y)` but never creates any `DisplayListItem`. The comment at line 755 says "TODO: Create proper text DisplayListItem" and line 757 says "This is a placeholder". The function always returns an empty `Vec`, making it non-functional.
- **Evidence**: Lines 744, 753 prefix variables with `_` (unused). The `items` Vec is never populated.
- **Recommendation**: Either implement the display list item creation or document this as unfinished and mark with `todo!()` so it fails loudly if called.

### [HIGH] Dead Code — most public types have zero external call sites
- **Location**: All types except `PageMargins`
- **Details**: Grep for each public type outside `fragmentation.rs` and `lib.rs` (re-export):
  - `BoxBreakBehavior` — 0 external uses
  - `BreakDecision` — 0 external uses  
  - `DynamicSlotContentFn` — 0 external uses
  - `FragmentationLayoutContext` — 0 external uses
  - `FragmentationDefaults` — 0 external uses
  - `PageFragment` — 0 external uses
  - `PageCounter` (from fragmentation) — 0 external uses
  - `PageSlot` / `PageSlotContent` / `PageSlotPosition` — 0 external uses
  - `PageNumberStyle` — 0 external uses
  - `KeepTogetherPriority` — 0 external uses
  - `decide_break` — 0 external call sites
  - `into_display_lists` — 0 external call sites
  - `generate_page_chrome` — 0 external call sites
- **Evidence**: `grep "decide_break\|generate_page_chrome\|into_display_lists"` across all `.rs` files only returns hits in `fragmentation.rs` itself. Types re-exported in `lib.rs:184-188` but never imported by any downstream code. `paged_layout.rs` only imports `PageMargins`.
- **Recommendation**: This entire module appears to be an unused parallel implementation. The actual paged layout system uses `solver3/pagination.rs` with its own `PageGeometer`, `PageTemplate`, and `PageMargins`. Either wire this module into the actual layout pipeline or remove it to avoid confusion.


### [LOW] `+spec` Comment
- **Location**: `fragmentation.rs:399`
- **Details**: `// +spec:block-formatting-context:a019b9` — noted, not suggesting removal.

## System Documentation
- System identified: **Paged Media / CSS Fragmentation** (part of the layout engine)
- Existing doc: None — no `paged-media.md` or `fragmentation.md` in `doc/guide/`
- Doc needed: A guide document explaining the paged media system: how `fragmentation.rs`, `paged.rs`, `solver3/pagination.rs`, and `solver3/paged_layout.rs` relate to each other, which is the current active implementation, and how page breaks / headers / footers work. This would help clarify the significant overlap between `fragmentation.rs` and `pagination.rs`.
