# Damage / Repaint Architecture — Refactor Plan

Status: **design** (2026-06-06). Scope: **X11 first** (CPU + GPU), then macOS/Windows/Wayland
(they share `cpurender` + the same `render_and_present` shape, so the model ports).

This document is the plan for fixing the family of redraw bugs (cross-window stale
header, cursor trails, brush-paint not showing, CPU scroll, GPU full-window flicker)
by replacing the current display-list-diff damage system with a **layout-level,
two-channel invalidation** model shared by every backend and both render paths.

---

## 0. TL;DR

1. **Stop diffing display lists.** Damage is produced *during restyle/relayout* from the
   per-node dirty set, in each node's **local coordinate space**, and projected to screen
   space through the scroll/clip/transform chain. The display list is still *built* for the
   renderer, but it is **not** the source of damage.
2. **Two damage channels, not one:**
   - `RenderDamage` → what the rasterizer must redraw (minimize raster cost).
   - `PresentDamage` → what region of the on-screen surface must be pushed (XPutImage /
     swap / buffer-age). **Defaults to the render damage, but is widened independently** for
     scroll (shift), GPU buffer-age, and "surface may be stale" cases.
3. **One damage detector for CPU + GPU** so bugs are fixed once. The renderer-specific code
   only *consumes* a `DamageSet`; it never computes it.
4. **A CSS-property → damage function** (`css_property_damage`) built on the *existing*
   `CssPropertyType::relayout_scope()` and `is_gpu_only_property()` — it answers "if this
   property changed on this node, what is the damage rect and is it raster or composite-only?"

---

## 0.5. BRUTAL HEADLESS TEST RESULTS (2026-06-06) — damage system is EXONERATED

Pure-Rust headless tests (`dll/src/desktop/shell2/headless/mod.rs` `mod tests`,
no X11) isolate the bugs decisively:

| Scenario | Damage produced | Verdict |
|---|---|---|
| Colored box recolor (non-text paint) | `Rects([100x50 @ box])` | ✅ pixel-exact, local |
| Colored box no-op relayout | `None` | ✅ no false positive |
| 200× no-op relayout | 197 µs each | ✅ fast |
| **Text content change** (AAA→BBBBBBBB) | glyph count stays `[3]` | ❌ **#11 stale text** |
| **Text no-op relayout** | `Rects([384x18.625])` (full line) | ❌ **#12 false-positive** |

**Conclusion: the damage machinery (`compute_display_list_damage`, `render_frame`,
the diff) is CORRECT.** Non-text damage is exact and no-op is clean. **Both redraw
bugs are entirely in the TEXT / glyph-shaping (IFC) generation path** — the display
list is built with stale glyphs (#11) that also diff non-deterministically (#12).

**Revised priority:** fix the text-generation bugs (#11, #12) FIRST — they are a
self-contained problem in the IFC/shaping/reconcile path, NOT in damage. The
damage refactor below (render/present split, GPU buffer-age, layout-level damage,
scroll shift) is still valuable for scroll/GPU/cursor correctness and is the
long-term target, but it is **not** the cause of the two reproduced bugs. Do not
refactor damage to "fix" a text-shaping bug.

Harness entry points: `make_window_with(state, layout_cb)`, `FrameDamage` (None |
Rects | Full) recorded on `CpuBackend.last_frame_damage`, `damage_area()`. Tests
are HONEST (assert correct behavior; failing test == reproduced bug).

### UPDATE — #11 FIXED (stale text)

Root cause: the display-list generator paints text from the **cached**
`inline_layout_result` (`display_list.rs` ~3539/3571), and `CachedInlineLayout::
should_replace_with` (layout_tree.rs) keyed cache replacement on **width only**.
So a same-width `RefreshDom` with CHANGED text computed the fresh (correct) glyphs
in `layout_flow` but **discarded** them — `should_store=false` — leaving the stale
cached layout for the generator to paint. (The IFC Phase 2d fast-path had the same
width-only blind spot.)

Fix (fc.rs `layout_ifc` + layout_tree.rs `CachedInlineLayout`): hash the
`inline_content` once per IFC pass (`inline_content_hash`), (a) skip Phase 2d
fast-path reuse when the hash differs, and (b) force the cache store/replace when
the hash differs even at unchanged width. `damage_text_change_repro` now passes
(glyphs `[3]`→`[8]`).

### UPDATE — #12 FIXED (false-positive damage)

Root cause: `DisplayListItem::is_visually_equal` (display_list.rs) handled the
common variants but fell to `_ => false` for the rest — including **`TextLayout`**
and **`HitTestArea`**, which appear in every text IFC. So a no-op relayout reported
those as "changed" → full-line damage every frame (defeating incremental render).

Fix: add two arms — `HitTestArea` paints no pixels ⇒ always visually equal;
`TextLayout` ⇒ equal iff same bounds/font/size/colour AND same layout `Arc`
(`Arc::ptr_eq`; the no-op reuses the cached Arc, a real change reshapes a new one).
`damage_noop_relayout_is_clean` now passes (no-op → `FrameDamage::None`).

**Both generation bugs fixed. Headless suite: 11/11 green.** Damage is now correct
for text change, no-op, box paint, and box no-op.

### Broadened brutal tests (14/14 green)

Added state-driven reflow/structural tests on the corrected baseline:
- `damage_box_size_reflow` — widen 100→200 ⇒ `Rects([200x50])` (exact). ✓
- `damage_reflow_shifts_sibling` — grow box1 ⇒ box2 shifts down; damage reaches
  the shifted sibling's new bottom (~158), no ghost. ✓
- `damage_structural_add_covers_new_node` — add a box ⇒ `Full` (conservative). ✓

**Known coarseness (→ #10 target, NOT a correctness bug):** the sibling-shift
damage is `384x158` — full *content width* (boxes are only 100 wide). Safe
(over-damage), but wasteful; precise per-node layout-level damage (the refactor)
would tighten the horizontal extent. Structural change is a blunt `Full` for the
same reason.

Still TODO (need event injection — a `step(HeadlessEvent)` harness helper):
cursor/hover move (old∪new 2-rect), caret blink, scroll (CPU shift +
present-whole-viewport). Then the damage refactor #10.

---

## 1. Empirical grounding (probe run, azul-paint, AZ_BACKEND=cpu)

Repro: right-click canvas → context menu → "Normal paint mode" (`metaball_mode = false`).
Expected: header button flips "Effect: Metaballs" → "Effect: Brush". Observed: **menu closes,
data mutates, but header stays "Metaballs".**

Instrumented `render_and_present` (`x11/mod.rs`) and `compute_display_list_damage`
(`cpurender.rs`). Key lines from the run:

```
[PAINT] layout(): metaballs=false                       # layout() DID re-run with new state
[RP] win=…802 regen=true want_redraw=true cpu=true       # parent DID regenerate (handle_event fix works)
[RP-CPU] win=…802 damage=Some(1) did_incremental=true    # 1 coalesced rect, incremental render taken
[DMG] item 2  differs … 624x480 @ (8,8)                  # canvas
[DMG] item 30 differs … 107.972x15.694 @ (42,141)        # "Effect: Metaballs" text
[DMG] old.len=53 new.len=53 n_diff=25 rects_after_coalesce=1 -> [624x480 @ (8,8)]
```

Findings:

- **A.** The cross-window *refresh* fix works: the parent regenerates after the child menu's
  callback (`handle_event` now fans `ShouldRegenerateDomAllWindows` out to all windows).
- **B.** A 1-button text change produced **n_diff=25** items and **full-window** damage
  (`624x480 @ (8,8)`) — because the button's width change reflowed the whole vertical button
  stack, shifting every later item. **DL-diff is both lossy and coarse.**
- **C.** Across the *entire* run, item 30's new bounds is **always 107px** ("…Metaballs") and
  **never ~82px** ("…Brush"). ⇒ **The display list never reflects the new text**, even though
  `layout()` ran with `metaballs=false`. This is a **second, independent bug** (stale glyph
  run), not a damage bug — see §11.
- **D.** Because render and present are coupled through `did_incremental`, an incomplete /
  stale raster is shown verbatim. There is no independent "present anyway" path.

Conclusion: the damage *layer* is wrong (B), and there is a separate stale-DL bug (C). This
plan fixes the layer; C is tracked separately.

---

## 2. Current architecture (what exists today)

| Concern | CPU path (`render_and_present`, `cpurender`) | GPU path (WebRender) |
|---|---|---|
| Damage source | `compute_display_list_damage(old_dl, new_dl)` → `Vec<LogicalRect>` (diff of flat lists) | WebRender's **own** internal `results.dirty_rects` |
| Render skip | `did_incremental`: empty rects + retained pixmap ⇒ **skip render, re-blit old** | `if !want_redraw && !scroll && !fade && !virtual_view { return }` (only when `!layout_was_regenerated`) |
| Present | **always** `XPutImage` whole pixmap (`0,0,pw,ph`) | **always** `swap_buffers()` (full) |
| Damage→present | (full blit; damage only gates raster) | `dirty_rects` → per-rect `Expose` in `request_redraw`; swap is full |

Flags: `frame_needs_regeneration` (relayout pending), `needs_redraw`/`want_redraw` (present
intent), `display_list_initialized`, `retained_pixmap`, `previous_display_list`,
`gpu_damage_rects`. Damage detection is duplicated **4×** (x11, macos, windows, wayland CPU)
plus WebRender's separate scheme — **bugs must be fixed in 5 places**.

Problems:
1. **No render/present separation.** Scroll cannot "render strip, present viewport"; GPU
   cannot "render dirty, present dirty with buffer-age" → full-window flicker.
2. **DL-diff is the wrong layer** (lossy, coarse, must build full DL every frame, breaks on
   scroll-frame coordinate spaces — see §4).
3. **Two damage detectors** (CPU diff vs WebRender) → divergent behavior, double bugs.
4. **False positives**: text items re-diff "different" frame-to-frame (sub-pixel / fresh
   glyph identity) → `n_diff` high every frame → "incremental" is effectively always full.

---

## 3. Core principle — two channels

```
            ┌── RenderDamage  ──► rasterizer (CPU fill+draw / WR render)
DamageSet ──┤
            └── PresentDamage ──► presenter   (XPutImage region / GL swap region / wl_damage)
```

- **`RenderDamage`**: regions whose *pixels* must be recomputed. Minimizes raster work.
- **`PresentDamage`**: regions of the *surface* that must be pushed to the compositor/screen.
  - Invariant: `PresentDamage ⊇ RenderDamage` (you must present whatever you re-rendered).
  - **Widened independently** by: scroll shift (present whole viewport), GPU buffer-age
    (present accumulated damage across N buffers), "surface stale" (first map, post-Expose,
    occlusion, window resize) → `PresentDamage = Full`.
  - **Correctness rule:** RenderDamage may legitimately shrink to ∅; **PresentDamage must
    never silently become ∅ when a present is required.** When unsure → `Full`. (This single
    rule kills the "lazy repaint ate my update" class.)

```rust
enum DamageRegion { Full, Rects(SmallVec<[LogicalRect; 8]>), None }
struct DamageSet { render: DamageRegion, present: DamageRegion }
```

`Rects` carries a small inline budget; exceeding `MAX_RECTS` (e.g. 16) collapses to `Full`
(bounded cost, never unbounded rect lists).

---

## 4. Where damage comes from — layout level, not the display list

**The display list is not diffed.** Damage is emitted by the engine that already knows what
changed:

- **Restyle** marks nodes whose computed CSS changed (the property cache already tracks this).
- **Relayout** marks nodes whose geometry changed (Taffy-style dirty flags;
  `relayout_scope` already classifies the blast radius).
- **Imperative** sources mark nodes/regions directly (caret, image-callback content, scroll).

Each dirty node emits damage in its **local coordinate space**, tagged with a `ChangeKind`,
then projected to screen space.

### 4.1 Coordinate spaces & scroll frames (the decisive reason to leave the DL)

A flat display list bakes scroll offsets into absolute item positions, so diffing positions
**cannot distinguish "content changed" from "the frame scrolled."** Layout-level damage keeps
each scroll frame as its own space:

```
screen_rect = project(local_rect, chain)
  where chain walks node → root, and at each ancestor applies, in order:
    - scroll offset   (subtract the frame's current scroll x/y)
    - transform       (2D/3D transform matrix, if any)
    - clip            (intersect with the frame's clip rect; empty ⇒ no damage)
```

- A node inside scroll frame F damages only the part of F's **viewport** it actually covers
  after clipping — never the off-screen scrolled-away region.
- **Scrolling is its own damage primitive**, not a diff artifact:
  - `RenderDamage` = the newly-exposed content strip (in F's content space, projected).
  - `PresentDamage` = F's whole viewport (screen space).
  - **CPU** additionally `XCopyArea`-shifts already-rendered pixels within F's clip by the
    scroll delta, then renders only the strip. (This is the "special mode" — shift, paint new
    strip, invalidate whole area — and is *impossible* without the render/present split.)
  - **GPU** sets the WebRender scroll offset (no re-raster of content) and presents F's
    viewport region.

This also means clean subtrees **skip display-list rebuild entirely** (perf, §8).

---

## 5. The CSS-property → damage function

Built on the **existing** `CssPropertyType::relayout_scope(node_is_ifc_member)` →
`{None, IfcOnly, SizingOnly, Full}` and `is_gpu_only_property()` → `{Opacity, Transform}`.
We extend, not replace.

```rust
enum DamageClass {
    Composite,   // GPU-only value (opacity/transform/filter): re-composite, NO re-raster (GPU)
    Paint,       // re-raster this node's visual_bounds (color/border/shadow/radius/bg/text-color)
    Reflow,      // geometry changed: damage = old_box ∪ new_box (+ repositioned siblings)
}

/// "If `prop` changed on this node, what must be redrawn?"
fn css_property_damage(
    prop: CssPropertyType,
    node_is_ifc_member: bool,
    old_box: NodeVisualBox,   // border-box + visual overflow (shadow), local space
    new_box: NodeVisualBox,
) -> (DamageClass, DamageRegion) { ... }
```

Mapping (derived from `relayout_scope` + `is_gpu_only_property`):

| Property group | relayout_scope | DamageClass | Damage rect (local) |
|---|---|---|---|
| `opacity`, `transform` (+`transform-origin`,`perspective`,`filter`,`backdrop-filter`,`mix-blend`) | None | **Composite** (GPU) / Paint (CPU) | `old_visual ∪ new_visual` (transform moves bounds) |
| `color`, `background-*`, `box-shadow-*`, `border-*-color`, `border-*-style`, `*-radius`, `cursor`, `caret-*`, selection-* | None | **Paint** | `visual_bounds` (shadow ⇒ expanded) |
| `border-*-width`, `padding-*`, `width/height`, `min/max-*`, `box-sizing`, scrollbar sizing | SizingOnly | **Reflow** | `old_box ∪ new_box` (this node + later siblings the parent repositions) |
| `font-*`, text layout props | IfcOnly | **Reflow** (IFC) | the **IFC container** bounds (the whole inline run reflows) |
| `display`, `position`, `float`, `margin-*`, flex/grid, `overflow`, `writing-mode`, … | Full | **Reflow** | containing block subtree (or parent's content box) |

Notes:
- **border-width vs border-style/color**: width is *Reflow* (box changes ⇒ `old∪new`); style &
  color are *Paint* (border-box edges only, content untouched). This is the exact distinction
  asked for.
- **box-shadow**: damage uses `visual_bounds()` (already expands by offset+blur+spread).
- **Composite (GPU-only)**: opacity/transform must **not** trigger CPU-style re-raster on GPU;
  they update a WebRender animated value and re-composite. On the CPU backend they degrade to
  Paint over `old∪new`. (`is_gpu_only_property` already names the set.)

---

## 6. Non-DOM / imperative damage sources

These have no CSS property and no DOM diff; they push damage directly:

| Source | RenderDamage | PresentDamage | Notes |
|---|---|---|---|
| **Caret blink/move** | `old_caret_rect ∪ new_caret_rect` | same | two rects; today caret forces broad repaint |
| **Mouse/hover cursor** (custom-drawn) | `old_cursor_rect ∪ new_cursor_rect` | same | the "2 rect areas" case — erase old, draw new |
| **Image/canvas callback** (azul-paint) | callback's reported dirty box, else node bounds | same | callback can return a sub-rect; default = node bounds |
| **Scroll** (see §4.1) | newly-exposed strip | **whole viewport** | CPU `XCopyArea` shift + strip; GPU offset + present |
| **Animation tick** (opacity/transform) | ∅ (GPU composite) | composited `old∪new` | drives off the GPU-value path |

---

## 7. Logical vs physical pixels (layout rule)

- Damage is computed and combined in **logical** pixels (the layout/CSS space).
- At the **presenter boundary** it is converted to **physical** device pixels:
  `phys = round_out(logical * dpi_factor)` — **round outward** (floor origin, ceil extent) so
  damage never under-covers a partially-covered physical pixel (prevents 1px seams/trails).
- The pixmap / GL framebuffer / `wl_buffer` are physical; `XPutImage`/scissor/`wl_surface_damage`
  take physical rects. `dpi_factor = window.dpi / 96.0` (already used in `render_and_present`).
- Clip and `XCopyArea` source/dest are physical and integer-snapped (X11 requires integers).

---

## 8. Performance ("quick enough")

- **No full DL rebuild for clean frames.** Layout-level dirty set ⇒ O(dirty nodes), not
  O(all items). Unchanged subtrees keep their cached display-list slice and cached glyph runs.
- **Bounded rects**: `SmallVec[..16]`; coalesce (existing `coalesce_damage_rects`, 8px gap);
  overflow ⇒ `Full`. Never an unbounded rect vector.
- **One pass, one detector**: damage built once per frame, consumed by both paths — no
  second WebRender-vs-CPU divergence to reconcile.
- **Composite-only fast path**: opacity/transform animations never touch the rasterizer.
- Avoid the current false-positive text re-diff (fix glyph-run identity, §11) so static text
  contributes **zero** damage frame-to-frame.

---

## 9. Unified consumption (CPU + GPU from one `DamageSet`)

```
DamageSet { render, present }
   │
   ├─ CPU presenter:
   │    scroll? → XCopyArea shift within clip (physical)
   │    render: render_display_list_damaged(display_list, pixmap, render_rects)   # clear+draw per rect
   │    present: XPutImage for each present rect (NOT always full)
   │
   └─ GPU presenter (WebRender):
        render: tell WR the dirty world rects (render rects) — drive its render(), don't rely
                solely on its internal diff
        present: query GLX_EXT/EGL buffer-age; accumulate present damage over the last `age`
                 frames; set swap damage region. No buffer-age ⇒ present = Full (correct,
                 kills flicker). Composite-only ⇒ no scene rebuild.
```

The renderer code **never computes** damage — it only reads `DamageSet`. That is the
"fix bugs once" property.

---

## 10. Proposed data flow

```
events/callbacks ─► restyle (prop cache dirty) ─┐
                 ─► relayout (geometry dirty)  ─┼─► DamageCollector
imperative (caret/cursor/scroll/image cb)     ─┘        │  per-node local rects + ChangeKind
                                                        ▼
                                          project through scroll/clip/transform chain
                                                        ▼
                                        DamageSet { render: …, present: … }  (logical)
                                                        ▼
                                   render_and_present(dpi) → physical → CPU/GPU presenter
```

New/changed types live in `azul-core` (shared): `DamageRegion`, `DamageSet`, `ChangeKind`,
`DamageCollector`; `css_property_damage` in `azul-css` next to `relayout_scope`.

---

## 11. Known separate bug (tracked, not in scope of the damage layer)

**Stale glyph run**: after a text-only DOM change, the rebuilt display list still carries the
*old* glyphs (probe finding C — "Effect: Brush" never appears in the DL). Suspect: glyph-run /
text-layout cache keyed by node-id rather than text content+style, so relayout reuses the old
shaped run. Must be fixed for the cross-window repro to go green **regardless** of the damage
refactor. Investigate: text shaping cache key in `layout/src/solver3` text/IFC path. Owner: TBD.

---

## 12. Implementation plan (incremental, each step verifiable on X11)

> Ground rule: do not regress the working cases (CSD titlebar, contenteditable caret, menus).
> Each step builds + runtime-verifies via `scripts/verify-menu-x11.sh` / `/tmp/verify_refresh.sh`.

- **P0 (done):** cross-window *refresh* — `handle_event` fans `ShouldRegenerateDomAllWindows`
  to all windows. ✅ verified (probe A). *Commit separately.*
- **P1:** fix the **stale glyph run** (§11) so a text change reaches the DL. Verify: item 30
  width flips 107→~82; header shows "Brush". (Unblocks the visible repro; independent of P2+.)
- **P2:** introduce `DamageRegion`/`DamageSet`/`ChangeKind` in `azul-core` + a `DamageCollector`
  that, for now, is *fed by the existing DL diff* but exposes the two channels. Make CPU
  present consume `present` rects (stop always-full `XPutImage`) and render consume `render`
  rects. No behavior change yet except correctness rule (present defaults Full when stale).
- **P3:** `css_property_damage` in `azul-css` (built on `relayout_scope`/`is_gpu_only_property`);
  wire restyle/relayout to feed the collector from the **dirty set** instead of DL-diff. Drop
  `compute_display_list_damage` from the hot path (keep behind a debug/fallback flag).
- **P4:** coordinate-space projection (scroll/clip/transform chain) + **scroll primitive**
  (CPU `XCopyArea` shift + strip; present whole viewport). Verify on a scroll demo.
- **P5:** GPU path — drive WR render from `render` damage, **buffer-age present** for `present`
  damage (fallback Full). Verify: no full-window flicker; opacity/transform = composite-only.
- **P6:** caret/cursor/image-callback imperative sources through the collector (2-rect cursor).
- **P7:** port the unified consumer to macOS/Windows/Wayland (delete the 4 duplicated CPU
  diff sites; one detector remains).

Open questions:
- Exact dirty-set API the property cache exposes today (does restyle already give per-node
  changed-property lists, or only a boolean?). Determines how thin `DamageCollector` can be.
- WebRender partial-present + buffer-age availability through the current GL context wrapper.
