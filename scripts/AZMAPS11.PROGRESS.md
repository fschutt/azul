# AZMAPS11 - AzMaps performance: requests off the UI thread, tiles-as-DOM cost

Worktree: .claude/worktrees/agent-aac0bc649ef9f07c2 (branch worktree-agent-aac0bc649ef9f07c2), based on
fix/input-bugs-2026-09-19 @ 86c0e821e. No cargo (the lead builds).

## Findings
- The tile grid's render (`map_widget_render`, UI thread) parsed and rasterised EVERY visible tile's SVG
  at 512x512 on EVERY render (`svg_string_to_dom` -> `render_svg_to_imageref`), and every fresh
  `ImageRef` was a new texture to register/upload. That is the 584 ms frame and the pan stutter.
- Labels were re-parsed from each tile's SVG on every render; up to 300 label nodes per frame.
- AzMaps' `on_viewport_changed` returns RefreshDom on every pan frame: a full app rebuild + a full
  re-render of the map view per pointer move.
- The tick timer has no interval (runs every frame) and rebuilds on every magnetometer reading.
- appkit `save_settings` reads/writes ~/.azlin/config.json on the UI thread (every kept viewport).
- Routing does not exist (ROUTING.md is a design note); no geocoding / place data requests exist.

## DONE
- (none committed yet)

## IN PROGRESS
- RED: map widget tests (drawn tiles, keys, marker, drawn cap).

## NEXT
- GREEN map widget + dll worker draws tiles; app RED/GREEN (overlay-only rebuilds, tick, route worker,
  --stats); appkit config write off the UI thread; E2E markers.
