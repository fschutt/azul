# HYGIENE progress (wave 5, 2026-10-02) - branch wt/hygiene from 2e92c759b

Brief: scratchpad/wave5/HYGIENE.md. Findings: scripts/DEDUP_WIDGETS_API_2026_10_02.md F1, F3, F4, F5, F17, F26, F31.

## DONE
- Item 1 (F1): 0a98cab8f - $crate:: inner macros, RefAny qualified, 78 hand imports dropped (page_breaks.rs left; its import is now unused -> parent drops it).

- Item 2 (F17): 7a71b84b9 RED, 539e896d3 GREEN - is_vec_family(), vecslice not structural; 14 types listed in the commit.
- Item 3a (F3): c32fbb52b - CoreCallbackData::create(event, refany, cb) in core/src/callbacks.rs; 8 private hook() gone, tile delegates.
- Item 3b (F4): 2dd113562 - decl layout section (simple, display_flex, grow, px_*...), timeline/cell_grid/dialog_kit/shells use it.
- Item 3c (F4): c0467267f - style_kit builders merged into decl (rename table in the commit), style_kit = theme marker only.

## IN PROGRESS
- Item 4 (F5/F31): one HTML escaper + one char-ref decoder in core/src/xml_html.rs; RED tests first.

## NEXT
- Item 5 (F26): layout micromail 0.1 -> 0.2.
- Report scripts/HYGIENE_2026_10_02.md.

## Decisions / open questions
