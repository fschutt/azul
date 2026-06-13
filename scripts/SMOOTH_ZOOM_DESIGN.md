# Smooth map zoom (translate/scale) — design (for review before build)

Status: **DESIGN — not yet implemented.** Requested 2026-06-10
("smooth zooming via translate()", "damage rects to cpu backend AND wayland
compositor", "interpolate smoothly via mouse scroll"). Build after sign-off.

## Today

`map_on_scroll` (`layout/src/widgets/map.rs`) adds `±0.5` to `viewport.zoom` per
wheel notch INSTANTLY, then re-renders the VirtualView grid. The render already
handles fractional zoom (`zoom_scale = 2^frac`, `tile_px = 256·zoom_scale`), so
the tiles ARE the right size — but each step is a hard jump, and a per-frame
animation here would **re-rasterise every tile SVG every frame** (cpurender runs
the SVG→pixmap path per tile), which is why a naive "animate the zoom value"
loop would stutter. Hence: transform-scale the existing layer.

## Proposed approach

Two-phase zoom, like every production slippy map:

1. **During the gesture (cheap, O(1)/frame):** keep the rendered tile grid at its
   current integer-zoom rasterisation and apply a CSS `transform: scale(s)
   translate(...)` to the GRID CONTAINER, where `s = 2^(animatedZoom - gridZoom)`,
   anchored at the cursor (zoom-to-point). No re-raster, no grid rebuild — just a
   transform on one node. A short ease (≈150 ms) interpolates `animatedZoom`
   toward the target accumulated from the wheel deltas.
2. **On settle (gesture idle ≈120 ms, or crossing an integer-z boundary):** commit
   `viewport.zoom = animatedZoom`, rebuild the grid for the new zoom (fresh tile
   set + eviction + fetch), and reset the transform to identity. The freshly
   rasterised tiles replace the scaled ones in one frame.

State on `MapTileCache`: `zoom_target: f32`, `zoom_anchor_px: LogicalPosition`,
`zoom_settle_deadline` (timer id). The wheel handler accumulates into
`zoom_target` + (re)arms a zoom timer (mirrors the existing scroll-physics timer);
the timer eases the transform each tick and fires the settle rebuild at the end.

## Damage propagation (the explicit requirement)

The transform scale changes the on-screen pixels of the WHOLE grid region every
tick, so the damage rect for a zoom tick = the grid's on-screen bounds (the
VirtualView region), NOT just a strip. Wire it like the existing VirtualView
damage:
- **cpurender**: a transform change on the grid node must damage the grid's
  visual bounds (extend `compute_virtual_view_damage` / the transform path so an
  animated `transform` on a VirtualView/child marks its bounds dirty). Verify the
  CPU renderer actually re-composites the scaled layer (it rasterises the child
  DOM at the VirtualView origin — confirm transform is honoured there, or apply
  the scale when compositing).
- **Wayland compositor**: that damage already flows to `wl_surface_damage` via the
  present path (the CPU branch damages the full surface / the GPU branch sends
  per-rect dirty), so once the frame's damage rect covers the grid region the
  compositor repaints it. Confirm the zoom tick's damage isn't dropped by the
  "nothing changed → skip" fast path (the transform value changed, so treat a
  changed `GpuValueCache`/transform as damage, like scroll does).

## Open decisions (confirm before build)

1. **Transform vs re-raster during gesture** — transform-scale (above, recommended)
   vs just animate `viewport.zoom` and re-raster per frame (simpler, but janky on
   the CPU backend). Confirm transform-scale.
2. **Zoom-to-cursor vs zoom-to-centre** — anchor the scale at the cursor
   (Leaflet/Maps default, recommended) or the viewport centre (simpler).
3. **Ease + settle timing** — ~150 ms ease, ~120 ms idle-settle; tune to taste.
4. **GPU vs CPU transform** — on the GPU backend the transform is a WebRender
   reference-frame (free); on CPU it needs the compositor scale. Both supported,
   but CPU is where AZ_BACKEND=cpu maps run — confirm cpurender honours a runtime
   transform on the grid node (may need a small render change).

Once confirmed I'll implement the timer + transform + the cpurender/Wayland
damage wiring, and verify on X11/Xwayland (xdotool wheel) + ask for a Wayland
re-test.
