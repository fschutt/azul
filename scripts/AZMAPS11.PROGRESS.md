# AZMAPS11 - AzMaps performance: requests off the UI thread, tiles-as-DOM cost

Worktree: .claude/worktrees/agent-aac0bc649ef9f07c2 (branch worktree-agent-aac0bc649ef9f07c2), based on
fix/input-bugs-2026-09-19 @ 86c0e821e. No cargo (the lead builds). Parse-checked with
`rustfmt --check` on copies in /tmp (no writes to the tree).

## Findings
- The tile grid's render (`map_widget_render`, UI thread) parsed and rasterised EVERY visible tile's SVG
  at 512x512 on EVERY render (`svg_string_to_dom` -> `render_svg_to_imageref`), and every fresh
  `ImageRef` was a new texture to register/upload. That is the 584 ms frame and the pan stutter.
- Labels were re-parsed from each tile's SVG on every render; up to 300 label nodes per frame.
- AzMaps' `on_viewport_changed` returned RefreshDom on every pan frame: a full app rebuild + a full
  re-render of the map view per pointer move.
- The tick timer had no interval (ran every frame) and rebuilt on every magnetometer reading.
- appkit `save_settings` read/wrote ~/.azlin/config.json on the UI thread (every kept viewport).
- Routing does not exist (ROUTING.md is a design note); no geocoding / place-data requests exist.

## DONE
- ea9752cab test(map) RED: drawn tiles, keys, marker, drawn cap (8 tests)
- 97fb65b72 fix(map): drawn once on the worker; render only places; keys; caps; own-view re-render; stats
- d72ea0e53 test(azmaps) RED: overlay-only rebuilds, route estimate, --stats
- d297c3639 fix(azmaps): overlay-only rebuilds, 100 ms tick, route worker, --stats
- d56857b84 fix(appkit): shared config written on the file thread
- fb490906d test(azmaps-e2e): --stats, drag without rebuilds, AZMAPS_ROUTE

## NEXT (for the lead)
- Build + run: layout lib tests (map.rs), AzMaps tests (model/args), dll build with map-tiles,
  scripts/azmaps_e2e.py.
- Follow-up: pins / route line as an overlay layer INSIDE the map's view (MapWidget API) so a pan with
  pins on screen needs no window rebuild either.
