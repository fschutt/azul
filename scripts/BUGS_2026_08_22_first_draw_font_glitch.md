# First-draw "font hinting goes bad" (AzWidgets, macOS Retina, CPU backend)

Investigation write-up, 2026-08-22. Read-only: no source was edited, nothing was
built. Evidence is code reading plus pixel forensics on the user's screenshot.

## Symptom

Verbatim: *"on the first draw sometimes the font hinting (backbuffer / font
hinting) goes bad"*. AzWidgets demo, macOS, Retina display, CPU (`cpurender`)
backend.

Screenshot (`Azul Widget Showcase.png`, 810x400 px = a 2x capture of the
window's top-left ~405x200 logical px):

- "Azul Widget Showcase" (26px bold #101828) — looks crisp at a glance.
- "Every built-in widget (callbacks fired so far: 0)" (13px #667085) — looks
  doubled / ghosted / smeared: every stem has a dark outline around a lighter
  core, and the weight is uneven along the line ("built" heavy, "-in" light).
- "Inputs" (section heading) and the "TextInput" label — crisp.
- "Type something..." (TextInput placeholder) — looks soft.

### What the pixels actually say

Zooming in (4x nearest) and dumping rows turns "ghosted" into something exact.
The "E" of "Every" has a 5-device-px stem:

```
184276 728195 7b8395 747370 300602      (fg = #667085, page bg = #f5f7f9)
```

- Left edge `(24, 66, 118)`: dark blue. Right edge `(48, 6, 2)`: near-black red.
- Both edge pixels are BELOW the text colour in EVERY channel. A src-over of
  fg=#667085 onto bg=#f5f7f9 can never produce a channel value outside
  `[fg, bg]`; these pixels are therefore not "fg blended onto the page". They
  are exactly what `fg` blended per-stripe onto BLACK gives: on the left edge
  the B stripe is most covered (B highest), on the right edge the R stripe
  (R highest) — the classic RGB-LCD left/right fringe pattern, but composited
  against (0,0,0) instead of the page.
- The core `(123,131,149)` is a normal ~0.87-coverage blend onto the page.
- No 2x2 pixel blocks anywhere: adjacent device pixels all differ. The text was
  rasterised at 2x. **This rules out a 1x-rendered-then-upscaled frame for this
  screenshot** (that would show blocky 2x2 blocks and 2-px-wide fringes).

The heading is the smoking gun. Its first two glyphs "Az" have the SAME
black-fringe signature (edges `000215`, `0d0f0a`, below fg #101828), while
"ul Widget Showcase" has ordinary light LCD fringes (`b08b5b` left, `4277a6`
right, between fg and bg). Two rendering flavours inside ONE text run.

Mapping "has any channel below fg" per column across the subtitle band
(y 112..136): every glyph of "Every built", "widget (callbacks...)" has such
pixels; the run "-in " (x 163..200) has NONE. Same two flavours, per glyph.

Text inside the white card ("Inputs", "TextInput", placeholder) never shows the
dark-fringe flavour: their cores/fringes are all inside `[fg, bg]`.

## Status

**Root cause identified with high confidence (candidate 1); not fixed.** It is
a CPU-renderer bug (all CPU backends, not macOS-specific), first-frame-only in
practice because the first frame is the only routinely LAYERED full render.
Two latent DPI-related bugs were found on the way (candidates 2/3); the
screenshot shows they are NOT what happened here, but they would produce a
"sometimes on the first draw" glitch on a multi-monitor / off-screen-created
window and deserve their own test. The remaining candidates from the brief
(glyph cache, double painting, contentsScale) are ruled out with evidence.

## Candidates, ranked

### 1. LCD subpixel text is swept against a TRANSPARENT compositor layer on the full-render path (CONFIRMED by pixel forensics)

Mechanism, step by step:

1. The first frame (and every other full repaint) goes through the layered
   compositor: `dll/src/desktop/shell2/headless/mod.rs:1168-1188`
   (`output.fill(white)` → `allocate_layers_from_display_list` →
   `render_layers` → `composite_frame`). `previous_display_list` is `None` on
   frame 1, so `dl_damage = None` and `is_incremental = false`
   (`headless/mod.rs:551-566`, `:738-800`).
2. The showcase column is `overflow-y: auto; height: 100%` (examples/
   azul-widgets/src/lib.rs:441-447) and overflows, so it emits
   `PushScrollFrame`. `allocate_layers_from_display_list` promotes every scroll
   frame to its own layer (`layout/src/cpurender/compositor.rs:180-200`).
3. `render_layers` clears every non-root layer to transparent black
   `(0,0,0,0)` (`compositor.rs:511-515`). The body's `#f2f4f7` background rect
   lives in the ROOT layer; inside the scroll-frame layer there is nothing
   under the heading/subtitle.
4. `DisplayListItem::Text` is painted with `force_grayscale = false`
   (`layout/src/cpurender/raster.rs:1822-1868`, the literal `false` at
   `:1865`). Default AA is LCD (`raster.rs:2531`, `TEXT_AA_DEFAULT = Lcd`; the
   "opt-in, off by default" comment at `:3170` is stale).
5. `render_text_with_bg` (`raster.rs:2847`) takes the pre-blended-tile path
   when the display list PROVED a uniform bg (`raster.rs:2891`
   `render_text_prerendered_lcd`). Singleton glyphs are OPAQUE COPIES of a tile
   pre-blended against the proven colour (`Pass 2b`, `raster.rs:3067`) — they
   come out right, because the proof colour IS the final backdrop. Glyphs
   whose 1-px-padded tiles overlap a neighbour form a component of >= 2 and are
   handed to the batch sweep (`let sweep_glyphs`, `raster.rs:3039`; `Pass 2a`,
   `:3048` → `render_glyphs_lcd`, `:2617`).
6. The sweep blends per stripe against whatever is in the destination row:
   `PixfmtRgba32LcdLinear::composite_pixel` reads `bg_lin` from the layer
   pixel (agg-rust-azul 1.1.3 `src/pixfmt_lcd.rs:720-724`) — which is black —
   and then forces the pixel opaque (`row[off+3] = 255`, `:749`).
7. `composite_frame` → `blit_pixmap` copies any `sa == 255` source pixel
   verbatim (`layout/src/cpurender/pixmap.rs:43-90`). The black-fringed glyph
   pixels land on the page unchanged.

Why it reads as "doubled / smeared": each stem gets a dark outline (blended
toward black) around a correctly blended core — a second, darker copy of the
glyph outline. Why it is per-glyph and uneven ("built" vs "-in", "Az" vs
"ul"): the tile/sweep split depends on whether adjacent ink boxes are closer
than 2 px (pad 1 + pad 1, `raster.rs:3015-3030`) — true for kerned "Az" at
52 ppem and for most pairs at 26 ppem, false for "-in" and "ul". Why 13px is
hit hardest: tighter spacing → more multi-glyph components → more sweeps. Why
text in the white card is fine: the card's white `Rect` is painted INSIDE the
same scroll-frame layer before the text, so the sweep blends against white.

The proof machinery is not wrong, it is answering a different question:
`compute_uniform_text_bg` (`layout/src/solver3/display_list.rs:2978-3020`)
walks layout ancestors across the scroll-frame boundary and proves the FINAL
backdrop; the sweep needs the LAYER-LOCAL backdrop, which the layered path
never provides.

Why "first draw": the incremental path (`render_display_list_damaged`,
`raster.rs:1311`) is FLAT — it clears each damage rect to white
(`raster.rs:1450`), paints the body rect, and treats `PushScrollFrame` as an
offset only (`raster.rs:2047-2060`), so the same sweep blends against the real
page and comes out right. Every later repaint that touches a run silently
fixes it (the counter bump that rewrites the subtitle, a hover, a resize
strip); the heading's "Az" survives because nothing damages it. Full (layered)
renders after frame 1 are rare: shrink resizes (`headless/mod.rs:468-474`),
`gpu_damage.needs_full`, structural item-count changes without patch damage.

Why "sometimes" (the part not proven, two plausible mechanisms):

- The font-cache snapshot race documented at
  `dll/src/desktop/shell2/common/layout.rs:258-270` ("incorrect font selection
  on some launches"): a different face/metrics changes which glyph pairs
  overlap (tile vs sweep), and a later relayout with the completed registry
  changes `font_hash` → every `Text` item differs → a flat repaint of all
  text → glitch gone. When the registry is complete before frame 1, nothing
  repaints and the glitch stays.
- `sync_window_size_from_content_view` (`macos/mod.rs:5827-5905`) only
  relayouts when the content-view bounds differ from the requested size; when
  they do, a GROW goes incremental (flat repaint of everything the reflow
  moved → fixed), a SHRINK recreates the compositor (layered again → still
  bad), equal sizes repaint nothing (still bad).

A secondary symptom to look for: after scrolling the showcase, the newly
exposed thin strip (`headless/mod.rs` `scroll_shift_region` + flat strip
repaint) renders with light fringes while the shifted content keeps the dark
ones — text weight visibly changes at the strip boundary.

Also affected, milder: grayscale text in a transparent layer.
`PixfmtRgba32::blend_pix` (`agg .../pixfmt_rgba.rs:110-115`) writes
`rgb = lerp(0, fg, a)` — i.e. PREMULTIPLIED colour with alpha `a` — and
`blit_pixmap` then multiplies by alpha AGAIN (straight-alpha src-over), so
edge pixels get `fg*a^2 + bg*(1-a)`: darker-than-correct edges for any
non-black colour. Same class of bug, same fix surface (layer alpha semantics),
lower visibility. Semi-transparent `Rect`s inside layers double-attenuate the
same way.

**What guards against it today:** nothing that covers this case.
- `render_layers_text_equals_plain_render` (`raster.rs:7744`) compares layered
  vs plain, but with the Text item in the ROOT layer and black-on-white — it
  cannot see a transparent-layer backdrop.
- `pretile_path_is_pixel_identical_to_the_sweep` (`raster.rs:7677`) proves
  tiles == sweep on an OPAQUE pixmap; true, and irrelevant inside a layer.
- `damaged_full_rect_text_equals_plain_render` (`raster.rs:7795`, task #17)
  compares damaged vs plain — both flat.
- The corpus damage-soundness gate (`layout/tests/e2e_fixtures/damage-sound.json`,
  the only `pixel_identity` fixture) has coloured boxes, no text, no scroll
  frame. The headless `incremental_vs_full` harness
  (`headless/mod.rs:5014`) would catch exactly this divergence, but no
  fixture puts LCD text inside an `overflow` container without a local bg.
- `render_text_shadow` (`raster.rs:3266`) already knows the rule ("the LCD
  per-channel path assumes an opaque background ... corrupts a shadow
  composited from a transparent layer") and forces grayscale for its offscreen
  — the same rule was never applied to compositor layers.
- Related history: "ghosted text in overflow:scroll content" was fixed once
  before by `skip_ranges` (`compositor.rs:1808-1813`, content drawn twice);
  `e677d2a1c` (task #17) fixed LCD fringe double-blend on damage repaints.
  Neither addresses the transparent-layer sweep.

**Fix plan (pick one primary, B is the smallest correct one):**

- **A. Do not allocate a layer for a plain scroll frame.** In
  `allocate_layers_from_display_list` skip promotion when the frame has no
  opacity/filter/transform — render its range flat into the parent with
  clip + offset, exactly like the incremental path does. The per-layer pixbuf
  only served `CompositorState::scroll_layer`, which has no callers outside its
  own tests (grep: only `compositor.rs:4912-4943`); `render_frame` already does
  thin-strip scroll shifting on the output pixmap. This makes full == flat by
  construction for the common case and also removes a full-frame pixbuf per
  scroll frame. Effort: 0.5-1 day incl. tests (watch `skip_ranges`,
  `find_matching_pop`, child-layer parenting).
- **B. Seed plain scroll-frame layers with the parent's backdrop.** In
  `render_layers`, for layers with opacity 1 / no filter / identity transform,
  copy the parent's already-rendered pixels under `layer.bounds` instead of
  `fill(0,0,0,0)` and composite them as opaque. Requires rendering parents
  before children (`self.layers` is a `HashMap`, `compositor.rs:35`, iteration
  order is unspecified — switch to a parent-first walk from `root_layer`).
  Effort: ~1 day. Keeps the layer structure; LCD blends against the true
  backdrop; opacity/filter layers unchanged.
- **C. Defensive: force grayscale for the sweep inside a transparent layer.**
  Thread `layer_is_transparent: bool` from `render_layers` →
  `render_display_list_range` → `render_single_item` → `render_text_with_bg`
  as `force_grayscale`. Tiles keep LCD (they bake the proven bg), sweeps go
  grayscale. Plus fix the premultiplied/straight mismatch so grayscale edges
  are right: either make `blit_pixmap` treat non-root layers as premultiplied
  or un-premultiply in the pixfmt. Effort: 0.5 day. Leaves a visible
  LCD-vs-grayscale weight difference between tiles and sweeps in the same run
  unless tiles are also forced to grayscale in that layer — so C is a stopgap,
  not the fix.
- **D. Invariant test ("ink gamut law")** regardless of A/B/C: for a solid-colour
  Text item on a proven solid bg, every pixel channel inside the run's ink rect
  must lie within `[min(fg,bg), max(fg,bg)]` (+-1 for rounding). This is what
  the screenshot violates and it holds for every correct LCD/grayscale blend.
  Add it to `assert_damage_sound`/the headless harness. Effort: 2-3 h.

### 2. DPI change between the first and second frame: a DPI-driven GROW keeps the old-scale pixels (LATENT, not this screenshot)

Evidence:
- Creation reads the scale from `window.screen()` with `unwrap_or(1.0)`
  (`dll/src/desktop/shell2/macos/mod.rs:4168-4173`, applied at `:4329`). A nil
  `screen` (window not yet on a display, or positioned off-screen by
  `position_window_on_monitor`, `:7774`) makes the creation-time layout and
  the first render 96 dpi. `windowDidChangeBackingProperties:` (`:2854`) →
  `handle_dpi_change` (`:4881-4960`) then relayouts at 192 and requests a
  redraw. `sync_window_size_from_content_view` (`:5827`) refreshes the DPI only
  when the DIMENSIONS changed (`:5866-5878`), so a same-size window keeps a
  stale DPI until the notification arrives.
- `render_frame` has no notion of DPI change. Same logical size at 2x is a
  pixel GROW, so `resize_grow_only` keeps the 1x pixels in the top-left
  (`headless/mod.rs:440-466`), `can_reuse_previous_frame` is true (`:528`,
  no DPI term), and the resize strips are computed from the OLD PHYSICAL dims
  divided by the NEW dpi (`:453-458`: `old_pw as f32 / dpi_factor`) — i.e.
  the "new" strips start at half the window. Layout is DPI-independent
  (`layout/src/text3` has no `dpi` anywhere), so the regenerated display list
  is visually equal and the diff is EMPTY → only those strips repaint; the
  top-left quadrant keeps the 1x frame (`:738-800` skip/incremental arms).
- On the macOS side, the first frame (1x pixmap) is nearest-neighbour
  upscaled into the 2x view framebuffer (`update_framebuffer`,
  `macos/mod.rs:2402-2418`); the next frame arms the view framebuffer as the
  native target (`:2433-2446`, `:6803-6830`) and the incremental repaint
  touches only the damage rects, so the blocky upscaled text survives
  everywhere else.

Expected look: blocky 2x2-pixel text and 2-px-wide colour fringes in the
undamaged region — NOT what the screenshot shows (its pixels are native 2x).
Triggers: external 1x monitor as primary + window opening on the Retina
display, window created while off-screen, dragging between displays
(partially covered by `e62ba4bbb`, which only made the redraw happen).

Guards today: none for the renderer (`incremental_vs_full` never changes
DPI); `e62ba4bbb` schedules the repaint but the repaint is incremental.

Fix: add `last_dpi_factor: f32` to `CpuBackend`; on change treat the frame as
a first allocation (recreate the compositor, `previous_display_list = None`,
`resize_preserved_pixels = false`) and clear `glyph_cache.lcd_tiles` is not
needed (keys carry ppem). On macOS, have `update_framebuffer`/`native_target_ptr`
refuse the partial path when the pixmap size changed since the last full copy
(they already fall back to full copy on size mismatch — fine — the problem is
purely the renderer's reuse). Effort: 2-3 h + one headless test (below).

### 3. The first render is raced by the `tickTimers:` one-shot (LATENT)

`set_window_ptr` schedules a one-shot `tickTimers:` 1/60 s after the pointer is
installed (`macos/mod.rs:2303-2313`); its handler calls
`render_and_present_in_draw_rect()` whenever `regeneration_pending()`
(`:1913-1924`), which is true right after creation. If it beats the first
`drawRect:`, `native_target_ptr` sees view size 0x0 and SETS the view size to
the render size (`:2433-2446`); `drawRect:` later recomputes the size from
`convertRectToBacking` (`:1543-1560`) and, if different, resizes the
framebuffer and fills it WHITE without re-rendering unless a dirty flag is up.
Benign when the creation-time DPI is right (sizes agree); combines badly with
candidate 2. Not the screenshot's cause (the frame is fully painted). Fix: make
the one-shot only `setNeedsDisplay` instead of rendering, or have `drawRect:`
render whenever it resized the framebuffer. Effort: 1 h.

### 4. Glyph cache serving a glyph rasterised for another scale/phase — RULED OUT

Keys carry everything scale-dependent: `GlyphPathKey{font_hash, glyph_id,
ppem}` and `GlyphCellKey{.., ppem, scale_fixed, subpx_x, subpx_y,
x_subsamples}` (`layout/src/glyph_cache.rs:53-84`); `ppem =
round(font_size * dpi)` and `scale = effective_px / upem` are recomputed per
draw (`raster.rs:3162-3168`, `:2919-2923`). `LcdTileKey` adds colour and bg
(`glyph_cache.rs:1642-1650`). A DPI change simply misses the cache. GC runs only
on idle frames (`headless/mod.rs:769-786`) and only drops the previous
generation. Hinting (`build_hinted_path`) is keyed by integer ppem with
`hint_correction` for fractional sizes — no size-threshold path that could
differ between 13px and 26px. Nothing here can produce channel values outside
`[fg, bg]`.

### 5. Something paints the text twice — RULED OUT

`TextLayout` is a no-op in CPU rendering (`raster.rs:1869-1876`) and
contributes no damage (`display_list.rs:1565-1585`). Text shadows only paint
when `text_shadow_stack` is non-empty (none in the showcase CSS). Child-layer
ranges are skipped in the parent (`compositor.rs:1808-1813`). The damaged
renderer merges overlapping rects so no pixel is blended twice
(`raster.rs:1372-1430`) and clips LCD writes to the stripe window (#17).
Double-blending would also keep pixels inside `[fg, bg]`; the screenshot's
edge pixels are outside it.

### 6. macOS present at the wrong contentsScale — RULED OUT

`CPUView` is not layer-backed (no `wantsLayer`, `macos/mod.rs:3543-3553`;
only `GLView` sets it, `:3521`). `drawRect:` wraps the backing-size bitmap in
an `NSImage` of `bounds.size` points and `drawInRect(bounds)`
(`:1585-1618`), which is 1:1 on Retina. The only rescale is the
nearest-neighbour copy on a pixmap/view size mismatch (candidate 2). A
wrong-scale present would show 2x2 blocks; the screenshot has none.

## How to verify

All headless, no OS window, `cargo test -p azul-dll --release` once the user
wants a build (release-only builds per the repo rule).

1. **Layered == flat for text in a scroll frame (fails today).** In
   `layout/src/cpurender/raster.rs` next to
   `render_layers_text_equals_plain_render`: display list =
   `[Rect(body, #f2f4f7), PushClip, PushScrollFrame, Text(13px, #667085,
   uniform_bg = Some(#f2f4f7, body rect)), PopScrollFrame, PopClip]`. Render
   through `allocate_layers_from_display_list + render_layers +
   composite_frame` and through `render_display_list_damaged` with one
   full-window rect (or `render_display_list`). Assert byte identity. Then
   assert the ink-gamut law on both. Run it with `AZ_NO_LCD_PRETILE=1` as well,
   which sends every glyph down the sweep (the heading would be bad in full).
2. **Showcase DOM through the headless window.** `make_window_sized` with the
   showcase's heading/subtitle/card structure, `regenerate_layout`, present one
   frame (`step`), then `incremental_vs_full`-style compare of
   `cpu_backend.last_frame` against a FLAT render of the same display list —
   note `incremental_vs_full` as written compares against a fresh
   `CpuBackend::render_frame`, which is itself layered and would agree with the
   bad frame; the reference for this bug must be the flat renderer.
3. **DPI change is a full repaint (fails today).** Headless: layout + present
   at dpi 96 (400x300), then `update_window_state(|ws| ws.size.dpi = 192)`,
   `regenerate_layout`, present; compare `last_frame` against
   `CpuBackend::new().render_frame(.., 2.0)` with `incremental_vs_full`. Expect
   0 differing pixels; today the top-left quadrant is the 1x frame and the
   resize strips are wrong (`old_pw / dpi_factor`).
4. **On the real machine, without a code change:** `AZ_DUMP_FRAME_DIR=/tmp/f`
   dumps `frame_000_full.png` — check the subtitle's stem pixels for channels
   below #667085 (the gamut law) to confirm the renderer, not the present, is
   at fault. `AZ_NO_LCD_PRETILE=1` should make the WHOLE heading dark-fringed;
   `AZ_TEXT_AA=grayscale` should remove the colour fringes (the edges stay a
   little too dark — the premultiplied blit). `AZ_NATIVE_BACKBUFFER=0` should
   change nothing. Dragging the window to a 1x display and back, or starting it
   on a 1x primary, exercises candidate 2.

## Effort

- Candidate 1 fix A (no layer for plain scroll frames): 0.5-1 day incl. the two
  tests above. Fix B (backdrop-seeded layers): ~1 day. Fix C (grayscale
  fallback + alpha semantics): 0.5 day, stopgap. Ink-gamut assertion: 2-3 h.
- Candidate 2 (DPI-aware `render_frame`): 2-3 h + test.
- Candidate 3 (tickTimers one-shot): 1 h.

## Overlaps

- Same fix surface as the memory note "OPEN: capture tile repaint (NullImage
  after ChangeNodeImage)" only in that both live in `render_frame`'s damage
  arms; no shared cause.
- Fix A/B interacts with the thin-strip scroll path and `scroll_fast_path_eligible`
  (`compositor.rs:1269-1340`): today the strip repaint is flat while the shifted
  pixels came from the layered render, which is the secondary symptom above;
  A removes that inconsistency for free.
- The premultiplied-vs-straight layer alpha issue (grayscale text, translucent
  rects inside opacity/filter layers) is a separate, broader fix; it does not
  block A.
- Candidate 2 touches the same `resize_grow_only`/`compute_resize_damage` code
  that `dd90d4938`/`b44804467` (this branch's slider/reconcile work) sit next
  to; no conflict, but the new `last_dpi_factor` field should be added in the
  same `CpuBackend` struct block (`headless/mod.rs:181-245`).
- The stale "opt-in, off by default" LCD comment (`raster.rs:3170-3172`) and
  the stale "TEXT paints under `real_clip_stack`" comment (`raster.rs:1554-1560`
  — the Text arm actually uses `clip_stack`) are worth fixing in the same PR so
  the next reader does not chase them.
