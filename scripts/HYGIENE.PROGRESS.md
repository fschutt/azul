# HYGIENE progress (wave 5, 2026-10-02) - branch wt/hygiene from 2e92c759b

Brief: scratchpad/wave5/HYGIENE.md. Findings: scripts/DEDUP_WIDGETS_API_2026_10_02.md F1, F3, F4, F5, F17, F26, F31.

## DONE
- Item 1 (F1): 0a98cab8f - $crate:: inner macros, RefAny qualified, 78 hand imports dropped (page_breaks.rs left; its import is now unused -> parent drops it).

- Item 2 (F17): 7a71b84b9 RED, 539e896d3 GREEN - is_vec_family(), vecslice not structural; 14 types listed in the commit.
- Item 3a (F3): c32fbb52b - CoreCallbackData::create(event, refany, cb) in core/src/callbacks.rs; 8 private hook() gone, tile delegates.

## IN PROGRESS
- Item 3b (F4): shared single-property layout helpers in themes/decl.rs; timeline.rs / cell_grid.rs / dialog_kit / shells `simple` use them.
- Item 3c (F4): merge style_kit builders into decl (decl names survive: fill=background; style_kit::fill() -> decl::fill_box();
  bg->fill, themed_bg->themed_fill, hover_bg->hover_fill, active_bg->active_fill, hover_border->hover_border_color,
  drop_shadow->themed_shadow, kit::border->themed_border, kit::focus_halo->focus_halo_stacked, focus_shadow_ring->focus_halo_inset_stacked);
  style_kit keeps the theme marker only.

## NEXT
- Item 3 (F4, F3): decl.rs + style_kit.rs merge; timeline/cell_grid private helpers; shared hook().
- Item 4 (F5, F31): one HTML escaper, one char-ref decoder, RED tests first.
- Item 5 (F26): layout micromail 0.1 -> 0.2.
- Report scripts/HYGIENE_2026_10_02.md.

## Decisions / open questions
