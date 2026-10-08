# BUTTONS13 - the blue button's hover jumps instead of fading

Branch: worktree-agent-a06e2e81a9c20bfab (fast-forwarded to cbd0ae0a8).
User: "the hover animation of the default blue button doesn't work it switches
immediately to the hover state without interpolating the gradient, glow, etc."

## Findings (trace)
- Seeding works: `LayoutWindow::seed_state_change_transitions` (layout/src/window.rs)
  starts background / border / shadow transitions on hover (ANIM8).
- Per-frame: `tick_animations` writes `from.interpolate(to, t)` as an override;
  gradients and shadows take the restyle path -> `ShouldUpdateDisplayListCurrentWindow`
  -> the shells rebuild the list (`regenerate_display_list_for_dom`). Repaint path OK.
- ROOT CAUSE: the interpolation (css/src/props/property.rs `CssProperty::interpolate`,
  css/src/props/style/background.rs `interpolate_background_layers`):
  * radial / conic gradient layers never pair -> flora's blue stone (6 layers, 3
    radial) holds its rest face, then jumps half way;
  * linear gradients whose stops MOVE (the stone's streak 6/15/22/32 -> 4/14/23/34)
    never pair -> jump;
  * colour <-> gradient never pairs (flat's dark Default: system:button-face ->
    hover gradient; flora's quiet: paper gradient -> wash) -> jump;
  * different layer counts (stone -> sunken stone) -> jump;
  * box-shadow / text-shadow have no interpolation at all -> the glow (gold rim +
    bloom), the pressed well and the focus ring jump half way.
- Declarations: flora.css gives `.btn-primary` / `.btn-hero-primary` background
  1.2s --fl-ease, border-color + box-shadow 1.2s `ease`; `.btn` (paper, quiet) 0.42s
  --fl-ease for background, border, color, box-shadow; `:active` 0.14s. Rust had the
  Illuminated (hero-primary) at 0.42s and the stone's edges/shadows on --fl-ease.

## DONE
- RED: css/src/props/property.rs `background_face_tween_tests` (moved stops,
  radial stone, colour<->gradient, cross-fade, layer fade in/out, from none, images) and
  `shadow_tween_tests`; layout/tests/a_hovered_button_passes_through_the_faces_in_between.rs.

## IN PROGRESS
- Engine fix: css background layers + shadows.

## NEXT
- Engine fix in css (background layers, shadows), flora durations, report.
