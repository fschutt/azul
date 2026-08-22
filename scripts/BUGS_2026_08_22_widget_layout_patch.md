# AzWidgets (macOS, Retina): TextArea bleeds into "Slider", placeholder jumps on hover, CheckBox off-centre

Date: 2026-08-22. Branch: `fix/open-bugs-wave-2026-08-22` (worktree `debug-slider-scroll-2026-08-22`).
Read-only investigation (no cargo run); evidence is code + pixel measurements of the three screenshots.
Demo source: `examples/azul-widgets/src/lib.rs` (`labelled()` 58-78, `section()` 80-97, TextArea at 120-129, Slider at 130-138, CheckBox at 155-159, `bump()` 467-475 returns `Update::RefreshDom` on every callback, initial `slider_value: 40.0` line 571).

## 1. Symptoms (verbatim) and what the screenshots actually show

All three screenshots are 2x (Retina); logical px = image px / 2. Measured with a per-row dark/blue pixel profile (scratch script, not committed).

### (a) "Multi-line text area bleeding into 'Slider'" — `Pasted Graphic 2.png` (1926x262)

| Feature | image rows / cols | logical, relative to the textarea box top |
|---|---|---|
| "TextArea" caption glyphs | y 19-36, x 54-152 | caption cap-top 17 px ABOVE the box |
| textarea top border (grey) | y 53-54 (1 logical px) | 0 |
| textarea left border | x 54-55 | — |
| placeholder "Multi-line text area..." glyphs | y 127-147, x 80-296 | cap-top +37, baseline +46.5 |
| "Slider" caption glyphs | y 163-178, x 54-118 | cap-top +55, glyph bottom +62.5 |
| textarea bottom border | y 179-180 | +63.5 (box is 127 px = 63.5-64 logical tall) |
| slider rail + blue thumb | y 199-230, x 1225-1620 | below the box |

So: the painted textarea box IS `min-height: 64px` tall (border-box), the placeholder sits at +37 (cap-top) instead of the ~+21 the CSS implies, and the next `labelled` block ("Slider" caption) starts INSIDE the box — its glyph bottom coincides with the box's bottom border.

Caption-to-caption pitch: "TextArea" cap-top y=19 -> "Slider" cap-top y=163 = 144 px = **72 logical**. With the demo's `labelled` wrapper (caption line ~14 + `margin-bottom: 6px` + widget + `margin-bottom: 16px`), the widget slot the flow reserved for the textarea is 72 - 14 - 6 - 16 = **~36 logical px**, not 64.

36 decomposes exactly: `1 + 4 + 13 + 13 + 4 + 1` = border + padding + (UA `<p>` margin-top 1em@13px) + (the same empty `<p>`'s margin counted a second time) + padding + border. I.e. the slot is the textarea's CONTENT-derived height with NO `min-height` clamp (and with a margin double-count, see 3.4), while the paint rect is the `min-height`-clamped 64. Two different heights for one node.

### (b) "Text 'Multi line text area' then jumps if I hover over slider" — `TextArea.png` (466x256, crop offset ≈ −12,−6 vs (a))

| Feature | rows | logical rel. box top | delta vs (a) |
|---|---|---|---|
| box top border, now BLUE `#4286f4` (= the widget's `:hover`/`:focus` border colour, text_area.rs:167-197) | y 47-48 | 0 | colour changed |
| placeholder glyphs | y 159-179 (x from 69) | cap-top **+56**, glyph bottom **+66** — BELOW the bottom border | moved DOWN 38 px = **19 logical**; x unchanged (±1) |
| "Slider" caption glyphs | y 157-178 | cap-top +55 | unchanged |
| box bottom border (blue) | y 173-174 | +63.5 | unchanged |

So the hover did not move the box or the neighbours; only the placeholder text run moved, by +19 logical px, and the textarea itself is in `:hover` (blue border) although the user was "hovering the slider". That is consistent with (a): the textarea's hit rect is its 64 px paint rect (`get_paint_rect`, display_list.rs:4557-4571; hit-test area pushed from the same rect), which now covers the "Slider" caption row, so a pointer travelling to the slider enters the textarea's hit rect.

### (c) "CheckBox not centered" — `CheckBox.png` (428x134)

| Feature | px | logical |
|---|---|---|
| outer box | x 46-85, y 61-100 (40 px) | 20 x 20 = 14 + 2x2 padding + 2x1 border (content-box) |
| border | 2 px | 1 |
| inner grey fill | x 52-67, y 67-82 (16 px) | 8 x 8 at +3,+3 from the outer edge = border 1 + padding 2 |

The fill sits at the top-left corner of the 14 px content box. Centred would be at +6,+6.

## 2. Status per symptom

| Symptom | Status |
|---|---|
| (c) CheckBox fill top-left | **ROOT-CAUSED (widget CSS)**, fix is a few lines, no engine work. |
| (a) textarea box vs. next sibling overlap | **ROOT-CAUSED to a class**: the flow slot is the unclamped content height (36) while the paint/hit rect is the clamped height (64). Exact path that drops the clamp is narrowed to two candidates (3.2, 3.3), both with file:line; needs one instrumented run to pick. Plus two independent widget/engine bugs that explain the "+18 px instead of +5 px" text inset (3.1) and the "13 counted twice" (3.4). |
| (a) placeholder drawn low inside the box | Partly explained (UA `<p>` margin adds 13 px to `top: 4px` — 3.1). The remaining +16 px in (a) and the +19 px jump in (b) are NOT explained by the positioning code as read; hypotheses ranked in 3.5. |
| (b) hover jump | **MECHANISM IDENTIFIED**: the frame before the hover and the frame after are produced by two different display-list builders over the same tree — a PATCHED `layout_document` build (splice + translate) vs. a WHOLESALE `regenerate_display_list_for_dom` re-emission — and they disagree about the placeholder run. Which of the two is wrong needs the A/B in §5 (`AZ_NO_DL_PATCH=1`). |

## 3. Findings (file:line evidence)

### 3.1 TextArea DOM/CSS — what the widget asks for

`layout/src/widgets/text_area.rs`

- Container (`dom()` 553-620): `div.__azul-native-text-area-container`, `contenteditable` + `with_dataset(RefAny::new(state))` (564-565, `RefAny::new` at 551), two children: `p.__azul-native-text-area-placeholder` (605-613) and `p.__azul-native-text-area-label` (614-619), each wrapping a bare text node.
- Container style `TEXT_AREA_CONTAINER_PROPS` 90-200: `position: relative` (91), `box-sizing: border-box` (93), `flex-grow: 1` (94), `min-height: 64px` (95-97, `MIN_HEIGHT_PX` 87), `padding: 4px` (103-112), `border: 1px inset` (114-156), `overflow-x: hidden; overflow-y: scroll` (158-159), `white-space: pre-wrap` (163-165), `:hover`/`:focus` border colours only (167-197 — paint-only, `RelayoutScope::None`, css/src/props/property.rs:1678-1690). No `display` → block.
- Placeholder style 225-237: `display: block; position: absolute; top: 4px; left: 4px` — **no margin reset**.
- Label style 202-213: `display: block; position: relative` — **no margin reset**.
- The UA sheet gives every `<p>` `margin: 1em 0` (core/src/ua_css.rs:248-256, mapping 584-585); 1em resolves against the element's own 13 px font (layout_tree.rs:1555-1567 `resolve_box_props(element_font_size)`) → 13 px top and bottom on BOTH `<p>`s.
- Consequence for the abs placeholder: positioning.rs resolves `top` set / `height` auto / `bottom` auto via rule 3, `final_pos.y = containing_block.y + top + used_margin_top` (positioning.rs:394-397, margins from 346-347), CB = the container's padding box (`find_absolute_containing_block_rect` 1182-1230). So the placeholder's border box starts at `1 + 4 + 13 = 18` px below the box top, and the value `<p>` at `1 + 4 + 13 = 18` as well. They coincide (good) but 18 px is not the 5 px the widget author intended; the same applies to `text_input.rs` (placeholder 540-600, label 447-490: no margin reset either, 11 px margins inside a single-line field).
- `set_placeholder_visible` 653-661 toggles `display: none/block` (+opacity) on focus/blur → `RelayoutScope::Full` on the placeholder → it becomes a `layout_root` on every focus change (solver3/mod.rs:686-705).

### 3.2 The flow slot ignores `min-height` — candidate A: the taffy leaf path

The textarea is a flex ITEM (`labelled` is `display:flex; flex-direction:column`, itself an item of the `section` column flex). Items of a taffy subtree are sized by `compute_child_layout` → `compute_non_flex_layout` (taffy_bridge.rs:1564-1690, 1719-2110):

- The leaf returns `size.height = known_dimensions.height` or `content + padding + border` (2010-2032); for the textarea's content (3.4) that is `26 + 8 + 2 = 36` — the leaf never clamps, clamping is taffy's job via `style.min_size` (taffy_bridge.rs:766-784, taffy-0.10.1 `determine_flex_base_size` flexbox.rs:801-831: `hypothetical = flex_basis.maybe_clamp(max(min_size, padding_border), max_size)`).
- `overflow-y: scroll` makes taffy's AUTOMATIC minimum 0 (`Overflow::maybe_into_automatic_min_size`, flexbox.rs:802-803), so if `min_size.height` arrives as `auto` the item is exactly 36 — the measured slot. With `min_size = 64` it would be 64.
- Two places can zero/drop it: (i) `get_css_min_height` returning `Auto` (compact-cache fast path getters.rs:795-815 `compact_u32_struct`, populated in core/src/compact.rs:189-192 from the cascade; slow path only for non-normal states), (ii) `should_suppress_cross_intrinsic` (taffy_bridge.rs:1243-1258, 1270-1330) which sets `style.min_size.height = 0` when it believes the PARENT is a ROW container — it reads the parent style through `translate_style_to_taffy_cached(parent_dom_id)` (660-671), which returns `Style::default()` (= `flex_direction: Row`) for a `None` dom id. For the demo the parent is the `labelled` div (column), so (ii) should not fire; listed because it is the only code that actively zeroes `min_size.height` and it is keyed on a potentially-default parent style.
- Net: a single debug run prints `[TAFFY compute_child_layout] node_idx=… flex_basis … size=…` (taffy_bridge.rs:1572-1585) and `[TAFFY CHILD RESULT] … used_size=…` (1435-1445) — that shows directly whether taffy produced 36 or 64 for the textarea on the cold pass.

### 3.3 The flow slot ignores `min-height` — candidate B: a flex item re-laid-out as a standalone layout root

This path is fully traceable and is exercised on EVERY `RefreshDom` in this demo:

1. `NodeDataFingerprint::compute` hashes the node's dataset by `RefAny` identity: core/src/diff.rs:1884-1890 `node.get_dataset().hash(..)` → `RefAny::hash` hashes `sharing_info` (refany.rs:616-620) → `RefCount` derives `Hash` over `ptr: *const RefCountInner` (refany.rs:152-157). `TextArea::dom()` allocates a fresh `RefAny` every build (text_area.rs:551, 565), and the TextArea has no merge callback, so both rebuild paths keep the FRESH allocation: `merge_fresh_dataset` `_ => fresh` (diff.rs:1157-1161, precascade path) and `transfer_states` (diff.rs:983-1095, full path, merge only with a callback). → `attrs_hash` differs on every rebuild.
2. `NodeDataFingerprint::diff` maps an `attrs_hash` change to `TAB_INDEX | CONTENTEDITABLE` (diff.rs:1940-1943); `CONTENTEDITABLE ∈ AFFECTS_LAYOUT` (diff.rs:85-91) → `needs_layout()` → `DirtyFlag::Layout` (cache.rs:1457-1490) → `recon.layout_roots.insert(textarea)` (cache.rs:2030-2043).
3. Root pruning only removes roots whose ANCESTOR is a root (cache.rs:1182-1197). Nothing promotes a dirty flex ITEM to its flex CONTAINER; `reposition_clean_subtrees` is a deliberate no-op for `Flex | Grid` parents with the comment "if a child is dirty, the parent would have already been marked as a layout_root" (cache.rs:812-816) — which is not true for this path.
4. Step 2 lays the textarea out standalone: `calculate_layout_for_subtree(textarea, cb = labelled's content box)` (mod.rs:1014-1100, `get_containing_block_for_node` 1605-1640) → `prepare_layout_context` → `calculate_used_size_for_node` applies `min-height` (sizing.rs:2141-2144, 2218-2228) → `used_size.height = 64`; content height is then skipped for scroll containers (`skip_expansion`, cache.rs:3003-3045). The siblings keep whatever taffy placed them at.
5. Paint/hit use `calculated_positions[idx] + used_size` (display_list.rs:4557-4571; `push_node_clips` 4316-4455 clips/scroll-frames the same rect). Result: a 64 px box painted into a slot taffy sized for the item — which only overlaps if taffy's slot was 36 (back to 3.2), OR if the slot came from a taffy run that measured the item at 36 because the item had been marked `intrinsic_dirty` and `taffy_cache`/`measured_content_sizes` cleared (mod.rs:708-735) and remeasured with the leaf's unclamped answer.

Either way, B is a real engine defect on its own (a dirty flex item must re-run its container's flex algorithm; the widget-state pointer must not count as a layout change), and it runs on every callback in this demo.

### 3.4 The "13 counted twice": empty `<p>` margin double-count in `layout_bfc`

fc.rs:1836-1885: for an EMPTY first child the self-collapsed margin is advanced into the pen — `main_pen += accumulated_top_margin + self_collapsed` (1859) — AND kept as `last_margin_bottom = self_collapsed` (1867). With a bottom blocker on the parent (the textarea's `padding-bottom: 4px`) the tail adds it again: `main_pen += last_margin_bottom` (2967-2969). The empty value `<p>` (empty text → `is_empty_block`) therefore contributes 26 px instead of 13 (CSS 2.2 §8.3.1: a collapsed-through box contributes its collapsed margin once). That is the `13 + 13` in the 36. Minor on its own, but it is what makes the measured number land exactly on "content, no clamp".

### 3.5 Why the placeholder text is low (+37 cap-top in (a)) and jumps (+19) in (b)

Known, explained part: 18 px = border 1 + `top: 4` + UA margin 13 (3.1) — expected cap-top ≈ +21. Observed +37 (a) and +56 (b). The extra +16 / +35 is NOT produced by any line of `position_out_of_flow_elements` as read (positioning.rs:144-690: CB origin + top + margin, interior re-laid-out every pass 607-690, children via `position_bfc_child_descendants` 690-700), nor by the painter (glyph y = content-box origin + glyph point, display_list.rs:5990-6030; `get_paint_rect` 4557). So the extra offset lives in one of the INPUTS: `calculated_positions[textarea]`/`[placeholder]` at Step 3.5 time, or the placeholder's `inline_layout_result` line y. Ranked:

1. **Two builders, two answers (the (b) jump).** A paint-only restyle (`apply_hover_restyle`, dll/src/desktop/shell2/common/event.rs:627-668 → `ShouldUpdateDisplayListCurrentWindow` → macos/mod.rs:195-198 `display_list_dirty` → build_atomic_txn 6630-6640) re-emits WHOLESALE from `layout_results[dom].{layout_tree, calculated_positions}` (window.rs:12492-12590; those are the post-layout tree/positions, window.rs:4224-4228, 4397-4408). Every `RefreshDom` frame instead goes through `layout_document`, where a structure-preserved reconcile with empty `css_dirty` takes the PATCH path (gate mod.rs:1321-1373: `last_reconcile_was_skipped || (structure_preserved && css_dirty.is_empty() && cascade_ctx_unchanged)`): `PatchState::build` (display_list.rs:2873-2930) re-emits only `reflowed_ifcs` + fresh + size-changed nodes and SPLICES everybody else's previous items translated by `new_pos − old_pos` of the owning layout node (`try_copy_cached_run` 3035-3075, `translate_item` 468). The two frames in (a)/(b) therefore come from different builders. The translate math is sound only if the old items were painted at `old_pos + o` and the new ones belong at `new_pos + o` with the SAME intra-node offset `o`; an abs box whose interior is re-run every pass (positioning.rs:607-690 overwrites `used_size`/`inline_layout_result`; `reflowed_ifcs` is only set when `layout_ifc` re-stores, fc.rs:3896-3912) is the node most likely to violate that. The A/B in §5 (`AZ_NO_DL_PATCH=1`) settles which builder is wrong without further reading.
2. **Pass-to-pass drift of the abs box's CB / static position under candidate B.** When the textarea is re-laid-out standalone (3.3), `process_out_of_flow_children` skips the placeholder because it is "already positioned" (cache.rs:2627-2634, `pos_contains` against `cache.calculated_positions.clone()`), so its interior is only refreshed by Step 3.5 — against a CB whose `used_size` just changed 36→64 (only matters for bottom/percentage insets, not for `top: 4px`, so this is a weaker candidate for the vertical offset, but it does make the `inline_layout_result` of the abs box a per-pass rewrite that the patch cannot see).
3. **Stale `relative_position` for the abs child from the taffy leaf path.** `position_flex_child_descendants` (cache.rs:3199-3260) seeds `calculated_positions[placeholder] = textarea_content_box + warm.relative_position` where `relative_position` for an abs child is never written by `layout_bfc` (abs children are skipped in both passes, fc.rs:1262-1270, 1600-1603) — so it is whatever an earlier pass left there. Step 3.5 overwrites the y for `top: 4px`, so this only matters if Step 3.5 is skipped (`find_absolute_containing_block_rect` `Err(_) => continue`, positioning.rs:229-238) or the node is filtered (`parent_is_flex_or_grid` 165-176 — not the case here).
4. Scroll-frame offset: the textarea is a scroll frame (`push_scroll_frame(clip_rect, content_size)` display_list.rs:4448-4455, `content_size = overflow_content_size` 6599-6603 ≈ 26 px < the 54 px clip). `ScrollManager::clamp` floors `max` at 0 (managers/scroll_state.rs:1404-1414), so a negative offset cannot come from the manager; listed only because it is the one mechanism that moves the placeholder text WITHOUT moving the border or the neighbours, which is exactly the (b) picture. Check the WR/CPU scroll-frame origin once, then drop.

The structural-identity DL cache (mod.rs:625-672) is NOT a candidate: the hover flips `state_hash` and thus the root subtree hash, so it cannot serve the stale list here.

### 3.6 CheckBox — `layout/src/widgets/check_box.rs`

- Container `DEFAULT_CHECKBOX_CONTAINER_STYLE` 110-176: `display: block` (114), `width/height: 14px` (115-116, content-box), `padding: 2px` (118-127), `border: 1px inset` (129-160), `cursor: pointer` (175).
- Content `DEFAULT_CHECKBOX_CONTENT_STYLE_{CHECKED,UNCHECKED}` 178-190: `width/height: 8px`, background, opacity — no margin, no `align-self`, and the container has no `align-items`/`justify-content`. A block child of a block container is placed at the content-box origin → +3,+3 from the outer edge, exactly what (c) shows. The field doc on `CheckBoxStateWrapper.inner` ("centered by default", 76) is aspirational.
- Working references: RadioGroup's ring `RADIO_GROUP_CIRCLE_STYLE` radio_group.rs:145-155 (`display: flex; justify-content: center; align-items: center`, 16 px ring / 8 px dot); Switch track switch.rs:138-143 (`display: flex; align-items: center; align-self: center`).
- Never centred historically (`git log -S"LayoutDisplay::Flex" -- layout/src/widgets/check_box.rs` is empty); not a regression.

### 3.7 Existing coverage

- `tests/e2e/widgets_headless_test.json` drives the widgets demo headlessly with `assert_screenshot` against `layout/tests/reference_images/widgets_headless/*.png` — those reference images do not exist in the repo (only `scrolling_headless/` and a few single-box PNGs), so it cannot be red today.
- `e2e/bug-slider-thumb-trail.json` + `dll/src/desktop/shell2/headless/mod.rs:5049 dragging_the_slider_leaves_no_thumb_behind` (b44804467, dd90d4938) cover the slider's patched-build DAMAGE, i.e. "the presented pixels equal a full repaint of the same display list" (`incremental_vs_full` 5012-5047). They do not compare the display list against the LAYOUT (a wrong-but-self-consistent list passes), and they mount the slider under a plain block body, not under the demo's `labelled`/`section` flex columns next to a `min-height` item.
- No e2e scenario mounts a `min-height` flex item in a nested column flex, nor an abs-positioned `<p>` inside a scroll container; no scenario asserts a widget rect across `regenerate_layout()` passes. `assert_layout` supports `x|y|width|height` (layout/src/e2e/full.rs:4695-4750); `mouse_move` exists (1832, handler 12097) and runs the real hover restyle (runner.rs:3170); `assert_damage_sound … pixel_identity` (8791-9075).
- The two fixes on this branch (dd90d4938 touches solver3/cache.rs + mod.rs patch-damage log, headless/mod.rs, cpu_backend.rs; b44804467 touches core/src/diff.rs dataset merge, common/layout.rs precascade path, slider.rs) do not touch positioning, the taffy bridge, fc.rs or the widgets in question. b44804467's `merge_fresh_dataset` is the exact spot where 3.3-step-1 happens for widgets WITHOUT a merge callback.

## 4. Root-cause hypotheses, ranked

1. (a) **Flex item sized by two authorities** — `min-height` clamped in the node's own `used_size` (sizing.rs via the standalone-root path 3.3, or taffy's final pass) but NOT in the slot its flex container reserved (taffy leaf measure 3.2 and/or a stale remeasure). Confidence high that this class is the cause (the 36 vs 64 numbers are measured); medium on which of A/B produces the 36 on the first frame.
2. (a)+(b) **Dataset-pointer fingerprint makes every `with_dataset` widget Layout-dirty on every rebuild** (3.3 steps 1-3). Confidence high (pure code reading, no branch depends on runtime state). It also means every `RefreshDom` produces a structure-preserved PATCHED build in which the textarea is a standalone layout root — the precondition for 3.5-1.
3. (b) **Patched-build vs wholesale-build disagreement for the abs placeholder run** (3.5-1). Confidence medium; decisive A/B exists.
4. (a) **UA `<p>` margins inside the text widgets** (3.1) — deterministic 13 px/11 px insets, wrong regardless of the engine bugs. Confidence high.
5. (a) **Empty-block margin double count** (3.4). Confidence high for the code path; impact small (13 px).
6. (c) **CheckBox content not centred** — widget CSS. Confidence certain.

## 5. Proposed fixes

### CheckBox (c) — `layout/src/widgets/check_box.rs`
Make the container a centring flex box like RadioGroup: add to `DEFAULT_CHECKBOX_CONTAINER_STYLE` `display: flex` (replace the `LayoutDisplay::Block` at 114), `flex-direction: row`, `justify-content: center`, `align-items: center`; the 8 px content then lands at +6,+6. (Alternative with zero layout-mode change: `margin: 3px` on both content styles, i.e. `(14 − 8) / 2`.) Keep `flex-grow: 0` on the content. Add a rect assertion (see §6).

### TextArea / TextInput widget CSS (3.1)
- Reset the UA margins on both `<p>` blocks: `margin: 0` (four longhands) in `TEXT_AREA_PLACEHOLDER_PROPS` (225-237), `TEXT_AREA_LABEL_PROPS` (202-213), and the three platform variants of `TEXT_INPUT_PLACEHOLDER_PROPS`/`TEXT_INPUT_LABEL_PROPS` in text_input.rs. The placeholder then sits at `1 + 4 = 5` px, coinciding with the value text at `padding-top`.
- Optional hardening: give the textarea an explicit `height` in addition to `min-height` only if 5.3 turns out to be slow to land (it sidesteps the clamp issue for this widget but hides the engine bug).

### Engine — fingerprint (3.3 step 1-2), `core/src/diff.rs`
- Stop hashing the dataset allocation: in `NodeDataFingerprint::compute` (1884-1890) hash the dataset's TYPE id (+ whether it is present), not `RefAny`'s pointer; or drop it from `attrs_hash` entirely (datasets are state, not layout).
- Do not map an `attrs_hash` change to `CONTENTEDITABLE` (1940-1943): split `attrs_hash` into `contenteditable/flags` (layout) and `dataset` (none), or classify dataset-only changes as `NodeChangeSet::DATASET` outside `AFFECTS_LAYOUT` (85-91).
- Interplay with b44804467: unchanged for widgets WITH a merge callback; for those without, the fresh allocation is still installed — that is fine once it is not part of the layout fingerprint.

### Engine — flex-item layout roots (3.3 step 3-4), `layout/src/solver3/cache.rs`
In the "Clean up layout roots" pass (1182-1197) promote any root whose parent's `formatting_context` is `Flex | Grid` to that parent (walk up while the parent is a flex/grid container), so the container re-runs taffy and re-places the siblings. This makes the comment at 812-816 true. Cost: a taffy run of the container per dirty item (it is already the behaviour for the root when the viewport changes).

### Engine — taffy leaf clamp (3.2), `layout/src/solver3/taffy_bridge.rs`
Only if the instrumented run shows 36 from taffy on the cold pass: (i) assert/log `taffy_style.min_size` for the textarea node in `translate_style_to_taffy` (766-784); (ii) make `should_suppress_cross_intrinsic` bail when `parent_dom_id` is `None` instead of reading a default (Row) parent style (1290-1300); (iii) as belt-and-braces clamp `final_height` in `compute_non_flex_layout` with the node's own min/max (2010-2032) the way `compute_taffy_scrollbar_info` already reads CSS — taffy then sees a hypothetical size that is already ≥ min.

### Engine — empty-block margins (3.4), `layout/src/solver3/fc.rs`
In the empty-first-child arm (1847-1867) either advance the pen by `self_collapsed` OR carry it in `last_margin_bottom`, not both; `layout/tests/margin_collapse_integration.rs` is the place for the regression test (`<div style="padding:4px"><p></p></div>` must be 4 + 13 + 4 tall at 13 px font).

### Engine — patch vs wholesale (3.5-1)
Once §6-A says which builder is wrong: if the patched build, add the abs-positioned nodes whose interior Step 3.5 re-laid (positioning.rs:607-690) to `ctx.reflowed_ifcs` when their `inline_layout_result` or `used_size` changed (or unconditionally: abs boxes are few), so `PatchState` re-emits them instead of translating; if the wholesale build, the bug is in the tree/positions themselves (3.5-2/3) and the fix belongs in positioning.rs.

## 6. How to verify

### A. Instrumented run of the real demo (no code change)
`AZ_RECON_DEBUG=1 AZ_FP_DUMP=1 AZ_PATCH_DEBUG=1 <AzWidgets>` then move the pointer over the slider once:
- `[recon] intrinsic_dirty += layout_idx N (dom D, flag Layout, …)` for the textarea container on every `RefreshDom` frame, and `[fp_diff] … old={attrs_hash: A} new={attrs_hash: B}` with only `attrs_hash` differing → confirms 3.3 steps 1-3.
- `[PATCHGATE] skipped=… preserved=true css_dirty=0 structure_ok=true … reflowed_ifcs={…} fresh={}` on those frames → the frame in (a) was a patched build (3.5-1).
- A/B: `AZ_NO_DL_PATCH=1` — if the text no longer jumps on hover, the patched build is the wrong one; if it still jumps, the tree/positions are.
- `AZ_TAFFY`-style output: enable `debug_messages` (the `[TAFFY compute_child_layout]` / `[TAFFY CHILD RESULT]` lines, taffy_bridge.rs:1572-1585, 1435-1445) on the cold pass and read the textarea's `used_size`: 36 → 3.2, 64 → 3.3.

### B. e2e scenario (new file `e2e/bug-textarea-min-height-flex-slot.json`, mounts the demo's structure without the widget)
```json
{ "op": "mount",
  "html": ["<div class=\"section\">",
           "  <div class=\"labelled\"><span class=\"cap\">TextArea</span>",
           "    <div id=\"ta\" contenteditable=\"true\"><p id=\"ph\">Multi-line text area...</p><p id=\"val\"></p></div></div>",
           "  <div class=\"labelled\"><span id=\"cap2\">Slider</span>",
           "    <div id=\"track\"><div id=\"thumb\"></div></div></div>",
           "</div>"],
  "css":  ["html, body { margin: 0; padding: 0; }",
           ".section { display: flex; flex-direction: column; padding: 18px; }",
           ".labelled { display: flex; flex-direction: column; margin-bottom: 16px; }",
           ".cap, #cap2 { font-size: 12px; margin-bottom: 6px; }",
           "#ta { position: relative; box-sizing: border-box; flex-grow: 1; min-height: 64px; padding: 4px; border: 1px solid #9b9b9b; overflow-x: hidden; overflow-y: scroll; font-size: 13px; white-space: pre-wrap; }",
           "#ta:hover { border-color: #4286f4; }",
           "#ph { position: absolute; top: 4px; left: 4px; }",
           "#track { display: flex; flex-direction: row; align-items: center; width: 200px; height: 16px; background: #cccccc; }",
           "#thumb { width: 16px; height: 16px; margin-left: 40px; background: #0d6efd; }"] }
```
then, with `wait_frame`/`wait` between steps:
1. `assert_layout #ta height 64` (tolerance 0.5); `assert_layout #cap2 y` ≥ `#ta y + 64 + 16` (read `#ta y` from `get_dom_tree`, or assert `#cap2 y − #ta y ≥ 80`); `assert_layout #ph y == #ta y + 18` (== `+5` after the margin reset, the scenario must pin one of them on purpose).
2. `snapshot_frame as before_drag`; `set_node_css_override` on `#thumb` `margin-left: 80px` (the slider's own mechanism, same as `bug-slider-thumb-trail`) → a structure-preserved PATCHED build; re-assert the three layouts; `assert_damage_sound vs before_drag pixel_identity` (the patched list must equal a full repaint of itself — and, new, the layout asserts prove the list's nodes are where the tree says).
3. `mouse_move` to the centre of `#thumb` (a real hover → `apply_hover_restyle` → wholesale re-emission); `wait_frame`; re-assert `#ph y`, `#ta height`, `#cap2 y`; `snapshot_frame as after_hover` and `assert_changed vs before_drag` must report ONLY the border-colour rect (a moved text run shows up as extra changed pixels).
4. `assert_work_bounded max_dom_regens 1` so the scenario is known to run on the incremental paths.

### C. Headless Rust test with the REAL widgets (next to `dragging_the_slider_leaves_no_thumb_behind`, dll/src/desktop/shell2/headless/mod.rs:5049; helpers `make_window_sized` 2912, `rects_by_class` 5309, `incremental_vs_full` 5012, `step`)
Layout callback = the demo's `section("Inputs", [labelled("TextArea", TextArea::create().with_placeholder(..).dom()), labelled("Slider", Slider::create(40.0, 0.0, 100.0)…dom())])` with `bump`-style `RefreshDom` callbacks. Then:
- `regenerate_layout()` ×2 ("initial" + "settle" — the second is the structure-preserved pass); read `rects_by_class("__azul-native-text-area-container")`, the slider caption span, `"__azul-native-text-area-placeholder"`; assert `ta.height == 64`, `caption.y >= ta.y + ta.height + 16`, `ph.y == ta.y + 18 (or 5)`, and that all three rects are bit-identical between pass 1 and pass 2 (this is the assertion that would have caught 3.3).
- `step(MouseMove over the thumb)`; assert the rects unchanged and `incremental_vs_full == 0`; then `step(MouseMove over the textarea)` (forces the `:hover` border) and assert again.
- CheckBox: `rects_by_class("__azul-native-checkbox-content")[0].origin == container.origin + (6, 6)` and size `8x8` after the fix (+3,+3 before).

### D. Unit tests for the engine pieces
- `layout/tests/margin_collapse_integration.rs`: empty `<p>` inside a padded parent contributes one collapsed margin (3.4).
- `layout/tests/flexbox_integration.rs` (it already has the `min-height: 0` flex child case at 690-700): add "column flex item with `min-height: 64px`, `overflow-y: scroll` and 26 px of content: its `used_size.height` is 64 AND the next sibling's `y` is `item.y + 64`", cold and after a `ChangeNodeCssProperties` no-op relayout.
- `core/src/diff.rs`: two `NodeData` that differ only in the dataset allocation produce a fingerprint diff with `needs_layout() == false`.

## 7. Effort

| Item | Effort |
|---|---|
| CheckBox centring + rect test | 0.5 h |
| TextArea/TextInput `<p>` margin reset + layout assert | 0.5-1 h (check the caret/selection geometry still lines up: `reshape_text_node` reads the container's first IFC) |
| Fingerprint: dataset out of the layout fingerprint | 1 h + run the reconcile tests (`core/src/diff.rs` autotests, `dragging_the_slider_leaves_no_thumb_behind`) |
| Flex-item layout-root promotion | 2-3 h (+ a frame_perf check: one extra taffy run per dirty item) |
| Taffy leaf clamp (only if A says taffy yields 36) | 1-4 h depending on which of (i)-(iii) |
| Empty-block margin double count | 1 h |
| Patch vs wholesale re-emit for abs boxes | 1-2 h after the A/B |
| e2e scenario + headless test | 2 h |

## 8. Overlaps

- dd90d4938 / b44804467 (this branch): same demo, same `RefreshDom`-driven structure-preserved patched builds; b44804467's `merge_fresh_dataset` is where the fresh dataset pointer is installed (3.3). The slider only escaped the same Layout-dirty churn because it got a merge callback.
- `scripts/SPEC_CONFORMANCE_REVIEW.md:715, 836` (min/max sizing partial conformance) and `scripts/SCROLLBAR_BUGS.md` (`overflow: scroll` reservations) touch the same sizing code.
- Memory note "suspected flex/grid bug where whitespace-only text may be counted as an item" — different symptom, same `layout_tree.rs:2100-2150` flex-children builder.
- `tests/e2e/widgets_headless_test.json` references missing reference images; worth regenerating once (a)/(c) are fixed so the widgets demo has a pixel baseline at all.
- Step 1.1 DL cache / `prev_dom_ptr` fast path: inspected (mod.rs:625-672) and ruled out for (b) — the hover changes the subtree hash.
