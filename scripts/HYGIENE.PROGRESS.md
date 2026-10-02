# HYGIENE progress (wave 5, 2026-10-02) - branch wt/hygiene from 2e92c759b

Brief: scratchpad/wave5/HYGIENE.md. Findings: scripts/DEDUP_WIDGETS_API_2026_10_02.md F1, F3, F4, F5, F17, F26, F31.

## DONE
- Item 1 (F1): 0a98cab8f - $crate:: inner macros, RefAny qualified, 78 hand imports dropped (page_breaks.rs left; its import is now unused -> parent drops it).

- Item 2 (F17): 7a71b84b9 RED, 539e896d3 GREEN - is_vec_family(), vecslice not structural; 14 types listed in the commit.

## IN PROGRESS
- Item 3 (F4/F3): reading decl.rs / style_kit.rs.

## NEXT
- Item 3 (F4, F3): decl.rs + style_kit.rs merge; timeline/cell_grid private helpers; shared hook().
- Item 4 (F5, F31): one HTML escaper, one char-ref decoder, RED tests first.
- Item 5 (F26): layout micromail 0.1 -> 0.2.
- Report scripts/HYGIENE_2026_10_02.md.

## Decisions / open questions
