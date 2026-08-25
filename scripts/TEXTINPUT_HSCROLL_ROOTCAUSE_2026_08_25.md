# Single-line TextInput horizontal caret-scroll — full root-cause chain (2026-08-25)

## Symptom (user report)
Typing a long string into a single-line `TextInput` does not scroll: the caret
freezes at the last visible glyph and newly-typed characters are invisible
("the text input is invisible when I type"). The field is append-only past its
right edge.

## The caret-reveal mechanism (works, once its precondition holds)
`LayoutWindow::scroll_selection_into_view` (layout/src/window.rs) reveals the
caret by:
1. anchoring on `text_edit_manager.multi_cursor.node_id` (the value `<p>` /
   IFC root, NOT the focused container),
2. `find_scrollable_ancestor(anchor)` — walks up looking for a node that is
   BOTH `warm.scrollbar_info.is_some()` AND registered in `ScrollManager`,
3. computing an instant delta and scrolling that node.

For this to fire, the value `<p>` must register as a **horizontal scroll node**
in `register_scroll_nodes` (layout/src/managers/scroll_registration.rs), whose
gate is `scrollbar_info.needs_horizontal`.

## DOM / layout shape (verified by probe)
`container(div, block) > placeholder<p> + value<p>(block, white-space:pre) > text`.
The container is a **block** (default div display resolves to Block, not Flex —
see layout_tree.rs:3328 / taffy_bridge.rs:171), so the value `<p>` **fills** the
container (394px in a 400px field) and does NOT overflow it. Therefore the scroll
must live on the **value `<p>`**, where the text overflows — not the container.
(My first attempt put overflow-x:auto on the CONTAINER; it was inert AND broke
click-focus. Reverted.)

## The chain of bugs found on the value `<p>` (overflow-x:auto path)
1. **Height collapse.** `overflow-x:auto` made the value `<p>` a scroll
   container; `cache.rs` `skip_expansion` (~3052) skipped content-height
   expansion for ANY scroll container (either axis), collapsing the single-line
   field to **used height 0**. FIX (WIP, applied): gate `skip_expansion` on the
   BLOCK axis only — `overflow-y` (physical) scroll/auto. A horizontal-only
   scroll container must still grow its height to the text line. After this the
   value is `used=394x13.2` (correct).

2. **overflow_size.width == 0.** `fc.rs:~3991` set `overflow_size` from
   `main_frag.bounds()`, which maxes over `layout.items`; under dense-text
   retention `.items` is an empty sentinel, so `bounds()` collapses to 0 and
   `needs_horizontal` is never raised. FIX (WIP, applied): use
   `main_frag.overflow.unclipped_bounds` (captured during line breaking to
   enclose every item; survives the sentinel swap), `max`'d with `bounds()`.

3. **THE REMAINING BLOCKER — text3 truncates the nowrap line.** With the above,
   the value box is `used=394x13.2` but `overflow_content_size` is still
   `0.0 x 13.2` and `unclipped_bounds` is `389.9 x 12.96` — i.e. the laid-out
   line is only ~box-width wide. The display list paints only **64 of 250**
   typed glyphs (`max_glyph_x=386.8`). So the `white-space:pre` line is
   **truncated to ~one box width during text3 layout** — the overflowing tail is
   never laid out (`overflow_items: []`, not clipped-but-present).

   The constraints reaching text3 are CORRECT:
   `white_space_mode=Pre, text_wrap=NoWrap, available_height=None`
   (read via `CachedInlineLayout.constraints`). And
   `break_one_line`'s `no_wrap` branch (cache.rs:~10306) consumes **all** items
   with no width check. Yet the produced line has only 64 items (≈394px). So
   either a DIFFERENT line-production path runs for this case, or something caps
   nowrap content at `available_width` upstream of / instead of the no_wrap
   branch. `extract_line_breaks`/`try_incremental_relayout` (cache.rs:6260+) are
   about incremental edits, not this. `find_optimal_breakpoints` (KP) is
   supposed to defer unbreakable/no_wrap content to the greedy `break_one_line`
   path (knuth_plass.rs:~502). **Next step: instrument text3's `layout_flow`
   entry to see which path runs for this IFC and where the 64-item cap comes
   from.**

## Reproduction (in-crate, fast)
`layout/src/e2e/runner.rs` test
`typing_past_the_right_edge_scrolls_the_text_input_to_keep_the_caret_visible`:
types ~250 chars into a ~394px field and asserts the value `<p>` registers as a
horizontal scroll node and the reveal advances `current_offset.x > 0`. Currently
FAILS at "register a horizontal scroll box" because of bug #3.
(Contains a temporary `PROBE2` eprintln block — remove once fixed.)

## What is NOT the bug
- CSS plumbing (white-space:pre → text3 Pre/NoWrap) is correct.
- The scroll/caret-reveal machinery is correct and wired.
- The container is not the scroll node (it's a block the value fills).

## Sibling follow-up noticed
`TextInput.max_len` (default 50) is NEVER enforced — typing past it is accepted
(known gap, text_input.rs:2080 / 3108). User flagged as the next bug.
