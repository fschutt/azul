# Review: layout/src/solver3/calc.rs

## Summary
- Lines: 208
- Public functions: 5 (`evaluate_calc`, `resolve_pixel_value`, `resolve_pixel_value_with_viewport`, `resolve_pixel_value_no_percent`, `resolve_pixel_value_no_percent_with_viewport`)
- Public structs/enums: 1 (`CalcResolveContext`)
- Findings: 0 high, 1 medium, 1 low

## Findings

### [MEDIUM] Known Bug Pattern — viewport unit fallback silently drops unit semantics
- **Location**: `calc.rs:184-187`
- **Details**: When `resolve_pixel_value` encounters `Vw/Vh/Vmin/Vmax`, it returns `pv.number.get()` raw — treating the numeric value as if it were pixels. This means `50vw` resolves to `50.0` regardless of viewport size. The comment says "fallback", but callers that don't use `resolve_pixel_value_with_viewport` will get silently wrong results. The `resolve_pixel_value_with_viewport` variant (line 192) handles these correctly, but it's only called from `sizing.rs:1679` and `sizing.rs:1761`. Other callers (e.g., `getters.rs:1973`, `getters.rs:2294`, `fc.rs:3234`) use the non-viewport variant and will get wrong results for viewport units.
- **Recommendation**: Consider making `resolve_pixel_value` return `Option<f32>` for viewport units (returning `None` when viewport context isn't available), forcing callers to handle the case explicitly rather than silently getting wrong values.

### [LOW] Code Style — while-loop with manual index could use iterator patterns
- **Location**: `calc.rs:74-109`
- **Details**: The main parsing loop in `evaluate_calc_ast` uses `while i < items.len()` with manual `i += 1` and special `i = j` jumps for brace handling. The brace-matching sub-loop (lines 89-100) similarly uses manual indexing. This is acceptable given the need to skip ahead for parenthesized sub-expressions, but the two-pass evaluation (lines 112-138 and 140-159) could potentially be cleaner.
- **Recommendation**: Minor — the manual indexing is justified by the recursive brace-matching logic. No action required.

## System Documentation
- System identified: yes — layout solver (`solver3`)
- Existing doc: none (no `layout-solver.md` or `solver.md` in `doc/guide/`)
- Doc needed: A `doc/guide/layout-solver.md` explaining the solver3 layout system, its key modules (calc, sizing, fc, taffy_bridge, getters), how CSS properties flow through resolution, and how calc() expressions are evaluated.
