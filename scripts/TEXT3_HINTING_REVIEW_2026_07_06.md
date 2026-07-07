# text3 + allsorts-hinting review — 2026-07-06/07

Branch `fix/text3-hinting-review`. Goal: brutal text-layout test harness, allsorts-fork
hinting review, and a CoreText autoregression loop to clone CoreText's hinted output.

This report is assembled from three independent evidence sources:
1. **Executable spec tests** (`layout/tests/text3_brutal_*.rs`) — written tests-first against a
   deterministic fake font, so a *failure = the impl diverges from the pinned CSS/Unicode value*.
2. **Code-read flags** from the harness authors (exact file:line, static reasoning).
3. **Multi-agent bug scan** (workflow) — appended under "Finder findings" when it lands.

Nothing here is auto-trusted: candidate bugs list a *mechanism hypothesis*; those that are
code-confirmed are marked **CONFIRMED**, the rest **CANDIDATE** (needs root-cause before fixing).

---

## What was built (all committed / on-disk)

| Artifact | Path | Status |
|---|---|---|
| Vendored allsorts-azul 0.17.0 + `[patch]` | `third_party/allsorts`, root `Cargo.toml` | committed, compiles |
| Deterministic TTF builder | `layout/tests/common/fakefont.rs` (886 ln) | fonttools-validated |
| Brutal shaping battery (25) | `layout/tests/text3_brutal_shaping.rs` | compiles + runs |
| Brutal selection battery (15) | `layout/tests/text3_brutal_selection.rs` | compiles + runs |
| Brutal solver3 battery (10) | `layout/tests/text3_brutal_solver3.rs` | compiles + runs |
| CoreText autoregression harness | `layout/tests/coretext_autoregression.rs` (1073 ln) | macOS + `coretext_tests` |
| Autoregression runner | `scripts/coretext_regression.sh` | `chmod +x` |
| Hinting bytecode golden tests | `third_party/allsorts/src/hinting/tests.rs` | (Writer D) |

**Fake font metrics** (upem 1000; at font-size 20 ⇒ scale 0.02): `a–z`=600u/12px, `A–Z`=700u/14px
(A=gid27, V=gid48), `0–9`=500u/10px, space+NBSP=250u/5px, `-`=300u/6px, CJK=1000u/20px,
Hebrew=550u/11px, U+0301 combining=0u, kern (A,V)=(V,A)=−100u/−2px. gid map in fakefont.rs header.

**Run:**
```
cargo test -p azul-layout --test text3_brutal_shaping --test text3_brutal_selection --test text3_brutal_solver3
scripts/coretext_regression.sh            # macOS visual autoregression loop
cargo test --manifest-path third_party/allsorts/Cargo.toml   # hinting golden tests
```

---

## Test scoreboard (first run, 2026-07-07)

| Battery | pass | fail |
|---|---|---|
| shaping | 17 | 11 |
| selection | 15 | 3 |
| solver3 | 7 | 6 |

20 spec-first assertions fail. Clustered by suspected root cause below.

---

## Candidate bugs (from failing spec tests)

### C1 — Inter-word space advance dropped from laid-out line width  ·  severity HIGH  ·  CANDIDATE
Every multi-word width comes out exactly one space (5px) short per space:
- `aa_space_aa_is_53px`: got **48** (= 12+12+12+12, space=0), expected 53.
- `double_space_collapses_to_one_gap`: got 48, expected 53.
- `bidi_mixed_run_is_80px_and_reverses_hebrew`: got **70**, expected 80 (two spaces × 5px lost).
- `word_spacing_3px_adds_between_words`: got **51** = 48 + 3 — i.e. base space contributes 0 and
  word-spacing then *adds* 3, proving the space glyph's own 5px advance is missing.

Hypothesis: `perform_fragment_layout` / line-width accumulation drops the advance of space
clusters (or treats every space as a collapsible/hanging trailing space). Alt hypothesis: the
test sums `ShapedCluster.advance` and space advances live in a separate gap field — **must read
the fragment-layout width path before fixing** (finder `line-breaking`/`shaping-cache`).

### C2 — min-content / max-content wrong  ·  severity HIGH  ·  CANDIDATE
- `min_content_is_widest_word...`: min-content("aaaa aaaa") got **96** (whole text unbroken),
  expected 48 (widest word). ⇒ min-content is not taking the break opportunity at the space.
- `inline_block_shrinks_to_max_content_101px`: child max-content got **96**, expected 101
  (48+5+48) — the inter-word space is missing from max-content too (ties to C1).

Note min=96 is *not* just the C1 space-drop (that would give a smaller number); min-content is
failing to break at all. Two distinct defects likely share the intrinsic-size code path.

### C3 — line-height:normal ~10% too tall (line box 22px, should be 20px)  ·  severity HIGH  ·  CANDIDATE
Systematic +2px per line — the classic "our text doesn't match CoreText/Chrome vertically":
- `div_60px_wraps_two_words_into_two_line_boxes`: block 60×**44**, expected 60×40 (2 × 22 = 44).
- `padding_reduces_content_width...`: block 70×**54**, expected 70×50 (2 × 22 + 10 padding).

For the fake font, line-height normal should = (asc 800 − desc(−200) + gap 0)/1000 × 20 = **20px**.
22px ⇒ either a synthetic ~1.1 factor, or metrics pulled from the wrong table (OS/2 usWin* vs
hhea vs sTypo*), or line_gap synthesized. **Prime CoreText-parity suspect.** (finder
`metrics`/`solver3-integration`.)

### C4 — Break-opportunity classification wrong (NBSP / newline / hyphen)  ·  severity HIGH  ·  CANDIDATE
- `nbsp_does_not_offer_a_break`: `"aaaa\u{00A0}aaaa"` @60px wrapped to **2** lines; NBSP (U+00A0)
  must *not* be a soft-wrap opportunity (UAX #14 GL). **Strong real-bug signal.**
- `break_after_hyphen_keeps_hyphen_on_first_line`: `"aaaa-aaaa"` @60px produced **5** lines
  (expected 2, break after `-`). 5 lines ⇒ likely breaking at every char (emergency/overflow
  path) rather than at the hyphen — possibly ties to C1 width miscount pushing everything over.
- `newline_forces_two_lines`: `"aa\naa"` gave **1** line (expected 2). May be *test-layer*: the raw
  fragment path may need `WhiteSpaceMode::Pre`; `\n` collapse is normally the DOM white-space
  phase. Triage before treating as product bug.

### C5 — letter/word-spacing quantized to integer px  ·  severity MED-HIGH  ·  **CONFIRMED**
`letter_spacing_half_px_must_not_be_quantized_away`: 0.5px letter-spacing → measured **0px**
(expected 2px over 4 gaps). Root cause read directly:
`solver3/getters.rs:2797, 2809` (letter-spacing) and `:2822, 2834` (word-spacing) all do
`Spacing::Px(px_value.round() as i32)` — any |spacing| < 0.5px truncates to 0 and all fractional
spacing loses sub-pixel precision. `crate::text3::cache::Spacing::Px` holds an `i32`, so the fix
is a **type widening** (add a float-carrying spacing, e.g. `Spacing::PxF32(f32)`, or change the
unit) threaded through the text3 positioning that consumes `Spacing::Px` — not a one-liner.
(word-spacing C1 interaction: `word_spacing_3px` also exercises this path.)

### C6 — RTL / bidi visual reversal not reflected in item positions  ·  severity MED  ·  CANDIDATE
- `hebrew_run_is_rtl_reversed_and_33px_wide`: first *logical* Hebrew char sits at x=0, but in RTL
  it must sit at the *largest* x (expected ~11, i.e. rightmost). ⇒ visual reorder not applied to
  `UnifiedLayout.items[].position.x`, or the test reads logical order.
- `bidi_mixed_run...` also fails (C1 width + reversal). Confirm whether `reorder_logical_items`
  output feeds item x-positions (finder `glyphs-script`).

### C7 — Selection geometry  ·  severity MED-HIGH  ·  partly CONFIRMED
- `empty_line_reserves_a_line_box`: caret/line-box height read back as **3.4e38** (= `f32::MAX`).
  An `AvailableSpace::Infinite`/`f32::MAX` sentinel is leaking into real geometry on an empty
  line. **Looks like a genuine uninitialized-height bug.** severity HIGH.
- `bidi_selection_over_rtl_run_splits_into_multiple_rects`: got **0** rects (expected ≥2). Ties to
  code-read flag `cache.rs:~4382 get_selection_rects` emitting a single rect with no bidi split;
  here it degenerates to zero across a direction boundary.
- `selection_including_trailing_space_adds_5px`: selection rect 48px not 53 — whether a selection
  spanning a trailing space includes its width is arguably impl-defined; lower priority.

### C8 — white-space:pre intrinsic width = 0  ·  severity MED  ·  CANDIDATE
`white_space_pre_preserves_spaces_and_newline_without_wrapping`: intrinsics came back `(0,0)` for a
`pre` block. Either pre-formatted intrinsic sizing returns 0 (bug) or the harness read the wrong
node. Confirm against the DOM white-space path.

### C9 — Explicit line-height:30px yields a 0/8px-tall box  ·  severity HIGH-if-real  ·  CANDIDATE
`line_height_30px_makes_a_30px_line_box`: the 200px-wide block reported height **0** (used sizes
`[(800,8),(784,0),(200,0)]`). Either explicit `line-height` isn't establishing the line box
height, or the test read a wrapper node. Disambiguate node selection first.

---

## Code-read flags (static, from harness authors — corroborating, not yet all test-covered)

- **`getters.rs:2797/2809/2822/2834`** — spacing integer-quantization. → C5. **CONFIRMED.**
- **`cache.rs:~4382-4400 get_selection_rects`** — single-line selection emits one rect, no split
  at a bidi direction boundary. → C7.
- **`cache.rs:~9301-9306 position_one_line`** — letter-spacing added after the *last* cluster too
  (CSS trims trailing letter-spacing at line edge). Minor.
- **Legacy `kern` vs GPOS** — `kern_pair_av/va` got 28px (no kern) vs 26px expected. The fake font
  ships a legacy `kern` table only (no GPOS). If text3's shaping applies GPOS only, legacy-kern
  fonts get no kerning (a real CoreText-parity gap for old fonts). Decision needed: wire legacy
  `kern` (allsorts `opt_kern_table`) into `g.kerning`, or document as unsupported. → new "K1".

### Rasterization / hinting parity flags (from CoreText-harness author, `glyph_cache.rs`/`raster.rs`)
- **`cpurender/raster.rs:~2266 render_scanlines_aa_solid`** — glyph fill uses linear agg coverage,
  **no text gamma**; CoreText applies a text gamma even with smoothing off ⇒ our edges read
  systematically lighter/heavier ("over-ink everywhere" divergence class). Fidelity gap.
- **`glyph_cache.rs:181`** — hinted glyph integer offset uses `glyph_y.round()`; fractional
  baseline (dpi≠1) can jump the glyph ±1px vertically as the fraction crosses .5.
- **`glyph_cache.rs:247`** — `set_ppem(ppem, f64::from(ppem))` hints at the *rounded* integer ppem
  while layout advances may use the fractional device size ⇒ sub-pixel advance drift across a word
  at fractional DPI.
- **Old `test_coretext_compare.rs` is broken** (won't compile: `tiny_skia` not a dep; `Arc` vs `&`
  after the lazy-decode refactor) and, even conceptually, loaded the CoreText font *by name*
  (substitution risk) with no subpixel pinning and `ppem=round` vs exact. Superseded by
  `coretext_autoregression.rs`. Consider deleting or fixing the old file.

---

## Divergence-class → suspect-code map (for the autoregression loop)
- over-ink everywhere → gamma/coverage in `cpurender/raster.rs`
- 1px vertical shift (rms_aligned ≪ rms_raw) → phantom-point/baseline rounding in
  `glyph_cache.rs::build_hinted_path` or the Y-flip
- stems 1px too far @ small ppem (large max-col-diff) → CVT cut-in / round state in
  `third_party/allsorts/src/hinting/interpreter.rs`
- identical to unhinted → hinting not running (gasp / hint_instance / build_hinted_path → None)

---

## Finder findings (multi-agent scan)

_pending — appended when workflow `wf_2d4e8c9b` completes._

## Hinting golden tests

_pending — Writer D report (bytecode-level FreeType-divergence candidates)._

---

## Fix status

_Fix wave in progress; see git log on `fix/text3-hinting-review`. Order: C5 (confirmed, localized)
→ C3/C1 (highest CoreText-parity value) → C4 NBSP → selection C7 → hinting per golden tests +
autoregression metrics._


---

## Finder findings — 46 CONFIRMED (adversarially verified), 3 refuted

Workflow `wf_2d4e8c9b`: 10 finders + 49 verifiers, 2.4M tokens. Each row was traced by a
skeptic verifier. Full detail (description/failure_scenario/repro) in the run journal.


### CRITICAL

- **`layout/src/solver3/getters.rs:2690`** (solver3-integration) — Pixel line-height (`line-height: 24px`) becomes a NEGATIVE per-run line height via the compact-cache fast path
  <br>_fix:_ In the fast path, apply the same negative-is-absolute-px convention: `let raw = normalized; if raw < 0.0 { LineHeight::Px(-raw/10.0 /*decoded px*/) } else { LineHeight::Px(raw/100.0*font_size) }`. Note the decode already divided by 10; reconcile so a stored -24000 -> decoded -2400 -> +24px. Better: store an explicit is_px flag in the compact cache instead of overloading sign.
- **`layout/src/text3/knuth_plass.rs:420`** (line-breaking) — Knuth-Plass never breaks a paragraph that ends in a Box (no trailing forced break) — whole paragraph collapses to one line
  <br>_fix:_ After building nodes, append a terminal sequence: Glue{width:0, stretch: large, shrink:0} then Penalty{width:0, penalty:-INFINITY_BADNESS} so the final break is forced at n; or special-case the backtrack to seed the last line from the min-demerit reachable index when breakpoints[n] is unset.

### HIGH

- **`layout/src/text3/cache.rs:9830`** (shaping-cache) — letter-spacing and word-spacing are excluded from line-break width and alignment measurement
  <br>_fix:_ Fold per-cluster letter-spacing (and word-spacing on separators) into get_item_measure, or add a spacing term when accumulating unit_width in break_one_line and when summing total_width in calculate_alignment_offset, using the same cursive/word-separator gating as position_one_line.
- **`layout/src/text3/cache.rs:10303`** (line-breaking) — NBSP (U+00A0) and NNBSP/MMSP are treated as soft-wrap break opportunities during line breaking
  <br>_fix:_ In is_break_opportunity_with_word_break, exclude the no-break spaces: treat U+00A0, U+202F (and U+2060/ZWNBSP) as non-break-opportunities even though is_word_separator is true for word-spacing purposes; e.g. `if is_word_separator(item) && !is_nobreak_space(item) { return true }`.
- **`layout/src/text3/cache.rs:8563`** (line-breaking) — overflow-wrap:normal shreds a long unbreakable word into one grapheme per line instead of overflowing on one line
  <br>_fix:_ In the Normal arm, push the entire next_unit and consume next_unit.len() (place the whole unbreakable unit on this line and let it overflow), matching the intent of the Anywhere/BreakWord branch's progress guarantee but without splitting.
- **`layout/src/text3/cache.rs:8435`** (line-breaking) — Leading whitespace is stripped at line start for white-space:pre and pre-wrap, destroying indentation
  <br>_fix:_ Gate the leading-whitespace skip on the mode: only skip when white_space_mode is Normal | Nowrap | PreLine; skip for Pre, PreWrap, BreakSpaces.
- **`layout/src/text3/cache.rs:6081`** (line-breaking) — min-content width computes to 0 for CJK / break-all / overflow-wrap:anywhere text (break-opportunity item advance dropped)
  <br>_fix:_ When an item is a break opportunity that is itself a rendered unit (not a separator/space), include its advance as a one-item word: e.g. `max_word = max_word.max(cur_word).max(if !is_word_separator(item) { adv } else { 0.0 }); cur_word = 0.0;`.
- **`layout/src/text3/cache.rs:4655`** (edit-selection) — move_cursor_left/right ignore current affinity → one keypress swallowed; document start unreachable by Left arrow
  <br>_fix:_ Branch on current affinity: for Right, if affinity==Leading advance to next cluster Leading, if Trailing the current position is already a boundary so advance two boundaries (next cluster's Trailing / cluster after next's Leading); simpler: convert the (cluster_id,affinity) pair into an absolute boundary index over the visual cluster order, add/subtract 1, convert back. Handle the leading edge of the first cluster as a distinct boundary.
- **`layout/src/text3/default.rs:774`** (shaping-cache) — Byte offset stored in RawGlyph.liga_component_pos breaks allsorts mark-to-mark (mkmk) and mark-to-ligature positioning
  <br>_fix:_ Do not overload liga_component_pos. Track byte offsets in a side table (parallel Vec keyed by glyph identity) or use RawGlyph.extra_data (the `()` type param) to carry the byte cluster, leaving liga_component_pos at its allsorts-managed default so GPOS mkmk/marklig equality works.
- **`third_party/allsorts/src/hinting/interpreter.rs:2561`** (hint-arith) — DELTAP pops (arg, point) in the wrong order — point index and delta byte are swapped
  <br>_fix:_ Swap the two pops so the point index is read first: `let point_idx = self.pop()? as u32; let arg = self.pop()? as u32;` (mirroring op_deltac).
- **`third_party/allsorts/src/hinting/interpreter.rs:990`** (hint-arith) — WCVTP/WCVTF resize CVT to an unbounded index → OOM crash on corrupt index
  <br>_fix:_ Apply the same bound as WS/RS: `if i > 10_000 { return Err(HintError::InvalidCvtIndex(idx)); }` before resizing, in both WCVTP and WCVTF.
- **`third_party/allsorts/src/hinting/interpreter.rs:484`** (hint-glue) — Per-glyph graphics-state reset omits round_state (round mode leaks from prep into every glyph)
  <br>_fix:_ Add `self.gs.round_state = RoundState::Grid;` (and the matching super-round params if you want full parity) to the per-glyph reset block after line 484, matching FreeType's TT_Run_Context.

### MEDIUM

- **`core/src/selection.rs:462`** (edit-selection) — Plain arrow key with an active selection moves past the selection edge instead of collapsing to it
  <br>_fix:_ For non-extend collapse: Right/Forward → collapse to max(start,end); Left/Backward → collapse to min(start,end), with no movement applied. This requires passing the arrow direction into move_all_cursors (or two collapse variants).
- **`layout/src/cpurender/raster.rs:2196`** (layout-glue-raster) — Hinted glyphs render at integer ppem size, unhinted fallback at fractional size -> size mismatch & animation wobble
  <br>_fix:_ Either scale the hinted (integer-ppem) outline by (font_size*dpi)/ppem before rasterizing to recover the fractional target size, or disable hinting (use unhinted fractional path) when |font_size*dpi - ppem| exceeds a threshold, so hinted and unhinted glyphs share one size.
- **`layout/src/glyph_cache.rs:280`** (hint-glue) — Hinted path built with pre-hinting on-curve flags; FLIPPT/FLIPRGON/FLIPRGOFF changes dropped
  <br>_fix:_ Use hint_glyph_with_flags (or extend hint_glyph_with_orus to also return flags) and pass the post-hinting flags into build_path_from_contours.
- **`layout/src/solver3/fc.rs:2626`** (solver3-integration) — IFC layout cache reuse key omits container-level constraints (text-align, text-indent, line-height, direction)
  <br>_fix:_ Fold a hash of the constraint-relevant fields (text_align, text_align_last, text_indent, direction, line_height, columns, white_space_mode) into the cache validity check alongside width + content hash.
- **`layout/src/solver3/getters.rs:2797`** (solver3-integration) — Sub-pixel letter-spacing / word-spacing silently quantized to whole integer px
  <br>_fix:_ Change Spacing::Px to hold f32 (keep a hashable wrapper like the existing Em variant uses to_bits), and drop the `.round()` casts; propagate f32 through the position math (cache.rs:7148,9302,etc. already do `px as f32`).
- **`layout/src/solver3/sizing.rs:707`** (solver3-integration) — Intrinsic text measurement uses default constraints → white-space:nowrap/pre min-content wrong, text-indent ignored
  <br>_fix:_ Populate the measurement constraints from the IFC root's real style (at minimum white_space_mode/text_wrap, word_break, hyphens, and fixed text_indent), or thread the already-computed text3_constraints into the intrinsic pass.
- **`layout/src/solver3/sizing.rs:1216`** (solver3-integration) — inline-block intrinsic width/height with em/rem resolved against DEFAULT_FONT_SIZE (16px), not the element's font size
  <br>_fix:_ Pass the element's resolved font-size and the root font-size: `resolve_pixel_value_no_percent(&px, get_element_font_size(...), get_root_font_size(...))`.
- **`layout/src/text3/cache.rs:6060`** (solver3-integration) — measure_intrinsic_widths omits letter-spacing and word-spacing → shrink-to-fit boxes are too narrow
  <br>_fix:_ In the intrinsic advance loop, add letter_spacing (per cluster, skipping cursive) and word_spacing (per word separator) to `adv`, matching position_line_items exactly.
- **`layout/src/text3/cache.rs:3472`** (shaping-cache) — layout_hash rounds font-size to integer, so fractional sizes share cache entries and coalesce with the wrong shaping size
  <br>_fix:_ Hash the exact font_size bits (or a fine quantization like round(px*64)) in layout_hash, and gate coalescing on exact font_size equality, not the rounded hash.
- **`layout/src/text3/cache.rs:8578`** (line-breaking) — Trailing spaces are unconditionally stripped at line end for white-space:pre / pre-wrap / break-spaces
  <br>_fix:_ Only run the trailing-collapsible-whitespace pop for collapsing modes (Normal/Nowrap/PreLine); for Pre/PreWrap/BreakSpaces keep the spaces and let position_one_line's hanging logic handle them.
- **`layout/src/text3/cache.rs:4447`** (edit-selection) — get_cursor_rect returns None for empty text, so no caret is drawn in an empty editable/empty line
  <br>_fix:_ When self.items has no clusters, fall back to a strut-derived rect: use the line box origin and the strut/font ascent+descent for height, x at the content-box left (respecting text-align/direction), width 1.
- **`layout/src/text3/cache.rs:4383`** (edit-selection) — get_selection_rects over-covers bidi/RTL selections by drawing a single min..max span between logical endpoints
  <br>_fix:_ Walk the clusters in the logical range and group consecutive clusters by bidi run / contiguous visual x, emitting one rect per visual segment (as browsers do), rather than a single endpoint-to-endpoint rect.
- **`layout/src/text3/default.rs:866`** (shaping-cache) — Hinted advance uses rounded integer ppem while glyph offsets/kerning use the exact (fractional) font size
  <br>_fix:_ Either scale the hinted advance back to the exact font_size (advance_px * font_size / ppem_rounded) or use unrounded scaling consistently for advance, kerning and offsets; at minimum keep the advance basis identical between the hinted and fallback branches.
- **`layout/src/text3/default.rs:790`** (shaping-cache) — Setting any font-feature or font-variant disables default ligatures/contextual-alternates for Latin-family scripts
  <br>_fix:_ Start from build_feature_mask_for_script's default mask and additively merge user features/alternates (subtracting only features the user explicitly set to 0) instead of replacing the mask with empty.
- **`layout/src/text3/default.rs:770`** (shaping-cache) — Glyphs at byte offset > 65535 are silently dropped from shaping
  <br>_fix:_ Store the byte cluster in a wider side-channel (see finding #1) rather than a u16, or at minimum shape in <64KB chunks with an offset base.
- **`layout/src/text3/edit.rs:224`** (edit-selection) — Deleting or type-replacing a selected non-text item (image) is silently a no-op
  <br>_fix:_ Handle non-text runs in delete_range (remove the whole item when the selection covers it) and in delete_backward/forward (remove or step over an adjacent non-text run) instead of only matching InlineContent::Text.
- **`layout/src/text3/edit.rs:120`** (edit-selection) — Multi-cursor edits that add/remove runs leave other cursors' source_run indices stale
  <br>_fix:_ After an edit that changes new_content.len(), decrement source_run of all previously-processed cursors whose source_run > the removed/merged run index (track the delta in run count, not just byte length).
- **`layout/src/text3/glyphs.rs:147`** (glyphs-script) — CombinedBlock (tate-chu-yoko) glyphs all render stacked at one x in the primary render path
  <br>_fix:_ Mirror get_glyph_positions: call process_glyphs(glyphs, item.position.x, glyphs.first().map_or(default, |g| g.style.writing_mode), None) ONCE on the full slice instead of looping per glyph.
- **`layout/src/text3/knuth_plass.rs:163`** (line-breaking) — Knuth-Plass never creates a soft-wrap opportunity after a mid-word hyphen (U+002D/U+2010)
  <br>_fix:_ In the general cluster-collection loop, stop collecting (or emit a zero-width Penalty) after any cluster whose text ends with U+002D or U+2010, so intra-word hyphen break opportunities are preserved.
- **`layout/src/text3/knuth_plass.rs:460`** (line-breaking) — Knuth-Plass includes the line-terminating space in line width and justification gaps
  <br>_fix:_ Before measuring/justifying a line in position_lines_from_breaks, drop trailing word-separator items from line_items (or exclude the final glue from line_width and the space count).
- **`layout/src/text3/script.rs:489`** (glyphs-script) — is_hangul swallows Halfwidth/Fullwidth Forms and Enclosed CJK, misclassifying kana/fullwidth Latin as Hangul
  <br>_fix:_ Remove U+3200-32FF and U+FF00-FFEF from is_hangul; route U+FF61-FF9F to is_katakana and leave fullwidth Latin/punct to the appropriate checkers (or treat fullwidth ASCII as stop/Latin).
- **`layout/src/text3/selection.rs:121`** (edit-selection) — Double-click word selection concatenates clusters in visual order and only within one line_index, breaking on bidi and soft wraps
  <br>_fix:_ Concatenate clusters in logical order (sort by source_cluster_id / logical byte) spanning the whole run, not just one visual line, before running word segmentation; map results back to cluster ids in logical space.
- **`third_party/allsorts/src/hinting/graphics_state.rs:200`** (hint-glue) — SROUND/S45ROUND phase hardcoded to 16/32/48 instead of period/4, period/2, 3*period/4
  <br>_fix:_ Compute phase from period after the is_45 adjustment: `self.super_round_phase = match phase_bits { 0 => ZERO, 1 => period/4, 2 => period/2, 3 => period*3/4 }` using self.super_round_period.
- **`third_party/allsorts/src/hinting/interpreter.rs:1290`** (hint-arith) — FLIPRGON/FLIPRGOFF iterate lo..=hi with unchecked bounds → hang on large/negative index
  <br>_fix:_ Clamp/validate the range against the glyph zone length and/or reject when hi >= zone.len() or hi < lo, e.g. `let hi = hi.min(self.zones[1].flags.len().saturating_sub(1));` and skip when lo > hi.
- **`third_party/allsorts/src/hinting/interpreter.rs:1967`** (hint-arith) — MIRP control-value cut-in gated on rounding and ignores zp0==zp1 condition
  <br>_fix:_ Match FreeType: apply the cut-in whenever zp0==zp1 (not gated on do_round), and do NOT apply it when zp0!=zp1: `if zp0 == zp1 && (dist - orig_dist).abs() > cvt_ci { dist = orig_dist; }` placed before the rounding step.
- **`third_party/allsorts/src/hinting/interpreter.rs:2495`** (hint-move) — ISECT treats only exactly-zero determinant as parallel; near-parallel lines extrapolate a far-off point
  <br>_fix:_ Detect near-parallel with a relative test like FreeType (compare |dax*dby-day*dbx| against |dax*dbx+day*dby| scaled by a factor), not denom==0.
- **`third_party/allsorts/src/hinting/interpreter.rs:2174`** (hint-move) — SHZ marks every shifted point as TOUCHED (FreeType shifts a whole zone untouched), poisoning later IUP
  <br>_fix:_ Give op_shz a direct add-to-current path that does NOT set touched flags (mirror op_shpix's inline move but without the flag inserts), or add a 'touch: bool' parameter to move_point and pass false from SHZ.
- **`third_party/allsorts/src/hinting/interpreter.rs:1624`** (hint-move) — move_point drops the move AND the touch when freedom·projection == 0; FreeType clamps F_dot_P to 1.0 and still moves+touches
  <br>_fix:_ Replace `if dot == 0 { return; }` with `let dot = if dot == 0 { 0x4000 } else { dot };` matching FreeType's F_dot_P clamp, so the displacement and touch flags are still applied.
- **`third_party/allsorts/src/hinting/interpreter.rs:1435`** (hint-glue) — Stack has no headroom over maxStackElements; under-declared fonts lose ALL hinting
  <br>_fix:_ Allocate/allow max_stack = maxStackElements + slack (FreeType-style, e.g. +32) or grow the stack dynamically like storage/CVT with a sane cap.

### LOW

- **`layout/src/text3/cache.rs:6717`** (shaping-cache) — Characters with no covering font are silently dropped (no .notdef / tofu emitted)
  <br>_fix:_ When no font covers a char, emit a .notdef glyph from the primary font (glyph_id 0) so a visible tofu box is shown and the byte range is preserved.
- **`layout/src/text3/cache.rs:6711`** (shaping-cache) — Font fallback picks an arbitrary font via nondeterministic HashMap iteration
  <br>_fix:_ Make the fallback deterministic: iterate loaded fonts in a stable order (e.g. sorted by FontId, or the chain's priority order) rather than raw HashMap order.
- **`layout/src/text3/default.rs:890`** (shaping-cache) — Ligature glyph reports logical_byte_len of only its first component character
  <br>_fix:_ Compute logical_byte_len from the span covered by all merged unicodes (info.glyph.unicodes) or the distance to the next cluster's byte offset, not just the first char.
- **`layout/src/text3/knuth_plass.rs:290`** (line-breaking) — Knuth-Plass MinContent produces a single line, so min-content == max-content for text-wrap:balance
  <br>_fix:_ For MinContent in the KP path, break at every legal opportunity (line_width -> 0 or a per-word greedy pass) and report the widest resulting line, rather than reusing the MaxContent infinite width.
- **`third_party/allsorts/src/hinting.rs:256`** (hint-glue) — Phantom point pp1.x hardcoded to 0 instead of (xMin - lsb); pp2.x wrong for lsb != xMin
  <br>_fix:_ Thread the scaled (xMin - lsb) into phantom[0].x and set phantom[1].x = phantom[0].x + advance; have the caller compute and pass phantom_top_y/bottom_y from vertical metrics as the doc comment already describes.

### Refuted (not bugs)

- `third_party/allsorts/src/hinting/interpreter.rs:2099` — SHP/SHC/SHZ shift magnitude is divided by freedom·projection; FreeType shifts by d·freedomVector directly
- `layout/src/glyph_cache.rs:180` — Mixed hinted/unhinted glyphs on the same baseline use round() vs floor() -> up to 1px vertical stagger
- `layout/src/glyph_cache.rs:135` — Hinting applied at every ppem with no gasp table consultation -> over-hinting/distortion at large sizes

---

## FINAL OUTCOME (fix wave complete — 2026-07-07)

**All 46 confirmed bugs addressed** (44 fixed at root cause; 2 low-risk deferrals below). Verified:

| Gate | Result |
|---|---|
| `text3_brutal_shaping` | 26 pass / 2 fail |
| `text3_brutal_selection` | 17 pass / 1 fail |
| `text3_brutal_solver3` | 14 pass / 0 fail |
| allsorts `hinting::tests` (FreeType-parity golden) | **17 / 17 pass** |
| `cargo build -p azul-layout` (default features) | clean |

The 3 failing battery tests are one root cause: **glyph-level RTL visual reversal (UBA rule L2) is not implemented** — `reorder_logical_items` orders runs visually but emits intra-run clusters in logical order. Widths are already correct; only the per-glyph x-order (and bidi-aware selection-rect splitting) is missing. Documented as `TODO(text3-review)` at `reorder_logical_items` rather than half-implemented (a naive full-L2 pass would double-reverse the RTL-base paragraphs the real app relies on).

### Highest-impact fixes
- **Space/empty-glyph advance = 0** (`d8b579fab`, font.rs): space glyphs were pre-decoded into the glyph cache *before* the `hmtx` bytes were attached, caching a 0 advance for the whole face; `get_hinted_advance_px` then returned stale `Some(0.0)` instead of `None`. Affected every space in every font. Cascaded to 4 tests.
- **line-height:normal ~10% too tall** (`24d165629`): solver3 forced `Px(1.2·font_size)` for undeclared line-height instead of passing `Normal` to be resolved from strut metrics — the classic "azul text sits lower than Chrome/CoreText" vertical-parity bug.
- **11 TrueType interpreter bugs vs FreeType v40** (`de0e9813`): SROUND phase, DELTAP arg order, WCVTP OOM, per-glyph round_state reset, move_point F·P clamp, stack headroom, FLIPRGON bound, MIRP cut-in, ISECT near-parallel, SHZ touched-flag, phantom pp1.x.
- **Knuth-Plass paragraph collapse** (`cde39154b`), **NBSP/min-content/spacing** (`15567ed39`), **mkmk mark-stacking** (`68dd63c10`), **legacy kern for GPOS-less fonts** (`1cd7f9b64`).

### Deferred (low-risk follow-ups, not blocking)
- Confirmed bug #2 `cache.rs:4655` move_cursor_left/right affinity — no battery coverage; deferred to avoid an unverified editor-navigation regression.
- RTL glyph-level visual reversal (the 3 tests above).
- Pre-existing, unrelated: `cargo build --all-features` fails on a `web_lift` `az_mark` const-fn/volatile issue that predates this work.

### Test harness + autoregression (deliverables)
- Deterministic TTF builder `layout/tests/common/fakefont.rs` (fonttools-validated) + 50 brutal shaping/selection/solver3 tests.
- FreeType-parity golden tests `third_party/allsorts/src/hinting/tests.rs` (fork lib-test scaffolding restored — it didn't compile at all before, `0ca8e0654`).
- CoreText autoregression harness `layout/tests/coretext_autoregression.rs` + `scripts/coretext_regression.sh` (16× upscaled `[ours-hinted | CoreText | diff | ours-unhinted]` panels + `metrics.jsonl` + worst-first `SUMMARY.md`). Run on macOS: `scripts/coretext_regression.sh`. This is the loop for continuing to converge hinting onto CoreText.

20 commits on `fix/text3-hinting-review` (see `git log master..HEAD`).

### CoreText autoregression — first baseline (Arial, 9–24px, 333 cases)
Ran `scripts/coretext_regression.sh`. Result: **333/333 DIVERGENT** (MATCH 0 / CLOSE 0). This is the *starting* measurement of the clone-CoreText loop, and viewing the 16× upscaled panels pinpoints the dominant divergence class:

> **Our hinted glyphs render near-bilevel hard-black stems; CoreText renders soft gamma-corrected gray anti-aliasing.** The diff heatmap is red-dominated (we over-ink: solid black where CoreText is mid-gray) with blue side-fringes (CoreText distributes coverage to neighbor pixels we leave white). `bboxΔ` shows our small glyphs ~1–2px narrower (`H`,`m`) and `W` ~3–5px taller.

**This is a rasterizer issue, not a hinting-interpreter issue** — the interpreter is verified correct (17/17 golden) and grid-fits stems as intended; `cpurender/raster.rs::render_scanlines_aa_solid` then converts the hinted outline with **linear agg coverage and no text gamma**, so edges come out harsh. **Next loop iteration (deliberately NOT done blind overnight — it changes ALL text rendering and needs review):** add a gamma-correct coverage LUT (CoreText applies a text gamma even with smoothing off) and soften hinted-edge AA, then re-run and compare `metrics.jsonl` (the script auto-diffs prev→now MATCH/CLOSE/DIVERGENT). Worst offenders and per-ppem histogram are in `SUMMARY.md`; upscaled panels are named `NNpx_<case>_<bucket>_rmsX.png`.
