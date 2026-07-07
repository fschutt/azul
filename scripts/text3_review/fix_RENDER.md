# Fix brief — cluster RENDER  (3 confirmed bugs)

Each bug was adversarially verified. Fix the ROOT CAUSE per the sketch; keep the
public API stable unless the fix requires it (note any signature change). After all
fixes, the crate MUST compile.

## 1. [MEDIUM] `core/src/selection.rs:462`
**Plain arrow key with an active selection moves past the selection edge instead of collapsing to it**

- **What:** In move_all_cursors, when extend_selection is false and the selection is a Range, it collapses to move_fn(&r.end) — i.e. it runs the movement function on the focus and uses that as the new caret. Standard editor behavior is that a bare Left/Right with a selection collapses the caret to the selection boundary in the arrow direction WITHOUT moving a character. This code instead advances one unit past r.end (Right) or one unit before r.end (Left), and it keys off r.end regardless of the arrow direction or the selection's logical direction.
- **Repro:** Given a single forward Range selection over "cat" (start = cluster before 'c', end = cluster after 't'). Dispatch a SelectionOp{ mode: Move, direction: Forward, step: Character } (Right arrow, extend=false). Expected caret: after 't' (collapse to end, no motion). Actual: move_fn(&r.end)=move_cursor_right(after 't') = one cluster into the following space/word. With direction: Backward (Left arrow): expected before 'c' (min boundary); actual move_cursor_left(after 't') = inside the word before 't'.
- **Fix:** For non-extend collapse: Right/Forward → collapse to max(start,end); Left/Backward → collapse to min(start,end), with no movement applied. This requires passing the arrow direction into move_all_cursors (or two collapse variants).

## 2. [MEDIUM] `layout/src/cpurender/raster.rs:2196`
**Hinted glyphs render at integer ppem size, unhinted fallback at fractional size -> size mismatch & animation wobble**

- **What:** ppem is rounded to an integer at line 2196, and build_hinted_path scales font units by compute_scale(ppem, upem) (glyph_cache.rs:252), so a successfully-hinted glyph is rendered at the INTEGER ppem size. The unhinted fallback path instead uses scale = (font_size_px*dpi)/upem (line 2195, glyph_cache.rs:201) i.e. the true FRACTIONAL size. Consequences: (a) within one text run, a glyph whose hinting fails (fallback) renders at a slightly different size than its hinted neighbors; (b) any fractional effective size (font-size:13.5px at dpi=1, or 15px at 125% Windows scaling -> 18.75) renders hinted glyphs at the wrong size (13.5px -> 14px, ~3.7% too large); (c) animating font-size makes hinted text snap between integer ppems, producing a visible size 'wobble' instead of smooth scaling.
- **Repro:** Call the text raster path with font_size_px=13.5, dpi_factor=1.0 on a TrueType font where some glyphs carry bytecode instructions and some don't (e.g. an instructed 'H' and an un-instructed glyph). ppem=round(13.5)=14. For 'H' build_hinted_path succeeds -> path scaled by compute_scale(14,upem) -> ~14px em; get_or_build_cells applies translation only. For the un-instructed glyph build_hinted_path returns None -> unhinted path scaled at render by scale=13.5/upem -> ~13.5px em. Observe: (a) the two glyphs have ~3.7% different heights; (b) the hinted glyph is ~0.5px taller than the 13.5px request; (c) sweep font_size_px 13.0->14.0 continuously and the hinted glyph height stays flat then jumps at the .5 boundary (ppem snap) instead of scaling smoothly.
- **Fix:** Either scale the hinted (integer-ppem) outline by (font_size*dpi)/ppem before rasterizing to recover the fractional target size, or disable hinting (use unhinted fractional path) when |font_size*dpi - ppem| exceeds a threshold, so hinted and unhinted glyphs share one size.

## 3. [MEDIUM] `layout/src/glyph_cache.rs:280`
**Hinted path built with pre-hinting on-curve flags; FLIPPT/FLIPRGON/FLIPRGOFF changes dropped**

- **What:** build_hinted_path calls hint_glyph_with_orus (which discards any on-curve flag changes) and then feeds the ORIGINAL raw_on_curve into build_path_from_contours. The interpreter's FLIPPT/FLIPRGON/FLIPRGOFF instructions mutate on-curve flags during hinting; hint.rs even provides hint_glyph_with_flags specifically to return the updated flags, but the caller does not use it.
- **Repro:** Take a TrueType font containing a glyph whose bytecode uses FLIPPT/FLIPRGON/FLIPRGOFF to change a point from off-curve to on-curve (or vice versa) at a small ppem (these ops target zone 1 / the glyph zone). Render that glyph through the hinted path: build_hinted_path -> hint_glyph_with_orus runs the program (mutating zones[1].flags via interpreter.rs:1286/1293/1303) but returns only positions; build_path_from_contours then reads the original raw_on_curve. At the flipped point the builder chooses the wrong segment (treats an off-curve control as a line endpoint or vice versa), so the contour kinks/bulges instead of matching FreeType/CoreText. Confirm by comparing against a call to hint_glyph_with_flags and diffing the returned flag vector against raw_on_curve for that glyph/ppem — the flags differ at the flipped index.
- **Fix:** Use hint_glyph_with_flags (or extend hint_glyph_with_orus to also return flags) and pass the post-hinting flags into build_path_from_contours.

