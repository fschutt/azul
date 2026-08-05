# LCD glyph atlas (P3) — implementation plan

**Status**: specified, not started. This is the next perf item after
`2c070a2a5`.

## Why this and not more micro-optimisation

Two bit-identical micro-fixes to the LCD blend landed back to back and
**neither moved the frame**:

| commit | change | full-frame idle |
|---|---|---|
| `321ae4889` | coverage tone LUT built once per gamma, not per text run | 20.6 → 20.58 ms |
| `2c070a2a5` | zero-coverage early-out ahead of six sRGB lookups | 20.58 → 20.89 ms |

Both removed real waste. Neither is measurable, and that is the finding:
**the remaining LCD cost is not per-pixel bookkeeping around the blend, it
is the volume of pixels blended.** Every further P1b item (fused
coverage/contrast/alpha table, gather-form FIR) optimises the same
per-pixel path this plan deletes, so they are on hold until this lands.

Measured phase split for a page of text (release, `perf`, ~7400 glyphs,
37 runs, 820×1000), after the cell cache and the sort fix:

```
composite_pixel      25.0 %   <- the blend itself
blend_solid_hspan    18.3 %   <- FIR distribution + span setup
sweep_scanline       15.8 %
sort_cells           10.9 %
quicksort+smallsort  13.5 %
add_cells_offset      5.2 %
```

Everything above the line is per-frame work to produce coverage values
that are **identical every frame for the same glyph at the same sub-pixel
phase**.

## What it is

Cache, per glyph, the **post-FIR RGB coverage mask**: a dense `w × h × 3`
byte array of per-stripe coverage plus its `(x0, y0)` origin. At draw
time the whole pipeline for that glyph becomes a blit of `w × h` pixels
through `composite_pixel` — no rasterizer, no cell sort, no scanline
sweep, no FIR.

This is valid because azul's glyph offsets are **whole pixels**
(`int_x * 3` stripes, `raster.rs`), with the sub-pixel phase already
baked into the cache key's bucket. A mask is therefore pixel-aligned and
correct at any integer offset.

### Measured (prototype, outside the repo)

- 44 ms → **6.59 ms** with the fused tables, 9.1 ms without → **5–7×**.
- Floor check: the same blit with the compositing arithmetic replaced by
  a subtract runs at 3.97 ms, so 6.59 ms is close to memory-bound. There
  is little left after this.

### Memory — it *reduces* footprint

Per (glyph, bucket), 95 ASCII × 3 buckets:

| ppem | cells | **mask** | ratio |
|---:|---:|---:|---:|
| 10 | 243 456 | 45 657 | **0.19×** |
| 15 | 356 640 | **85 539** | **0.24×** |
| 24 | 560 928 | 192 918 | 0.34× |
| 32 | 745 632 | 323 598 | 0.43× |
| 48 | 1 106 304 | 682 479 | 0.62× |
| 64 | 1 471 824 | 1 184 073 | 0.80× |

Cheaper than the cell cache it replaces at every size tested — ~300 B per
entry at 15 px. Extrapolated to a 2000-entry working set: ~0.6 MB of
masks against ~2.5 MB of cells, so **~1.9 MB back** against the 20–30 MB
RSS target. Per-frame transient allocation drops to zero (today ~310 KB
live per run × 37 runs of churn).

Add a **ppem cap** (fall back to cells above ~64 px) so headline text
cannot invert the ratio.

## Output difference — small, and it is a fix

Prototype diff against today: **366 differing bytes of a 3.28 MB
framebuffer, max delta 37.** All of it one case: when a glyph has two
spans within 4 stripes on one scanline, today's code composites them as
two separate src-over passes, double-darkening the overlap. The mask sums
the FIR once and composites once — **the physically correct answer for a
stripe panel**.

So this is not bit-identical, and it must not pretend to be. Ship it
behind a flag next to `AZ_LCD_BLEND`, diff the reftests, eyeball the
diffs, then flip the default and re-baseline if needed.

Overflow is bounded: the five taps at any output stripe come from five
*distinct* source stripes, so the maximum is
`tert(255)+sec(255)+prim(255)+sec(255)+tert(255) = 251`. Verified for
azul's weights (`0x56/0x4D/0x08`, summing to exactly 256).

## Shape of the change

**agg-rust** (branch `perf/lcd-span-scratch-buffer`):

1. A mask type — `LcdCoverageMask { w: u16, h: u16, x0: i16, y0: i16, cov: Vec<u8> }`,
   `cov.len() == w * h * 3`.
2. A builder that runs sweep + FIR once for a rasterizer's current
   contents and returns the mask.
3. `blend_lcd_mask(&mut self, mask, x, y, color)` on both LCD pixfmts,
   blitting through the existing `composite_pixel`.

**azul**:

4. `GlyphCache` gains a mask map beside the cell maps, in the same
   generational scheme (`glyph_cache.rs`), keyed exactly like
   `GlyphCellKey` with `x_subsamples: 3`.
5. `render_glyphs_lcd` (`raster.rs`) blits masks instead of accumulating
   cells, behind the flag, with the cell path kept as the fallback for
   >64 ppem and for `AZ_TEXT_LCD=0`.

## Tests it must carry

- **Bit-comparison against the cell path** on a corpus of glyphs at
  several ppem and all three sub-pixel buckets, asserting the ONLY
  differences are the overlapping-span case above. A blanket "close
  enough" tolerance would hide a real regression.
- **Memory**: assert mask bytes < cell bytes for the same working set at
  15 px, so the claim above stays true.
- **The ppem cap actually falls back** — negative-control it by forcing
  the cap to 0 and requiring the cell path to be taken.
- Reftests: 45/52, and any movement reviewed image by image rather than
  re-baselined blindly.

**Every one of these needs its control run and seen to fail.** Three
negative controls this session were no-ops that passed against broken
code; see `azul-gates-with-wrong-premises` in memory.

## Background-colour keying (user request, follow-up)

The user asked whether the atlas can be cached against the background
colour so a patch repaint can blit composited glyphs rather than
re-blend. **Check before relying on it**: the linear-light path blends
against the *actual background pixel*, not a nominal colour, so a
(fg, bg) key is only valid where the background under the glyph is
uniform. That needs a "is this run over a flat background?" test, or it
will produce wrong fringes over gradients and images. Treat as a separate
step after the mask cache itself is correct.
