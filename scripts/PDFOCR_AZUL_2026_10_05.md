# PDFOCR-AZUL - report (2026-10-05)

Branch `wt/pdfocr-azul`, base `4158b7039`. Two engine bugs pdfocr found rendering
Calmet vol. 1 through azul's HTML layout + printpdf
(/Users/fschutt/Development/pdfocr/results/engine-issues/README.md, issues 2 and 3;
issue 1 - sup/sub/vertical-align - and issue 4 - printpdf - are other agents').
Nothing compiled (house rule): the coordinator builds and runs the commands below.

## Commits

| commit | what |
|---|---|
| 7f90585b5 | progress file |
| a8e5b0167 | test(text): RED - hyphens: auto hyphenates in the language of the lang attribute |
| 030ce1de7 | fix(text): GREEN - hyphens: auto hyphenates in the content language of the lang attribute |
| 0221a4aca | progress |
| 92001bbee | test(layout): RED - a relatively positioned inline box moves its text |
| 794987ebb | fix(layout): GREEN (1/2) - one rule for a relative shift, and the shift of an inline box's content |
| 93706ab82 | fix(layout): GREEN (2/2) - a relatively positioned inline box paints its text moved |
| 919a2032d | fix(layout): GREEN - a moved inline run claims no proven background |

## To register (coordinator) - two NEW test files, `layout/tests/all.rs` untouched

```rust
#[path = "hyphens_auto_hyphenates_in_the_language_of_the_lang_attribute.rs"]
mod hyphens_auto_hyphenates_in_the_language_of_the_lang_attribute;
#[path = "a_relatively_positioned_inline_box_moves_its_text.rs"]
mod a_relatively_positioned_inline_box_moves_its_text;
```

Both use `crate::table_markup` (already registered); the second also the paged
entry `layout_document_paged_with_config` like `inline_block_text.rs`.

## Issue 3 - `hyphens: auto` ignored the `lang` attribute

**Root cause, two halves.**
1. `core/src/xml_attributes.rs`, the ONE attribute table every XML loader reads
   (core's `str_to_dom` - printpdf's path - the streaming loader, the code
   generator): it had no `lang` entry, so `lang="en"` never reached the DOM at
   all (`setting_of` returned `None`, the attribute was dropped). `<html lang>`
   was dropped too, though `html_root_node_data` already applies the root's
   attributes.
2. `layout/src/solver3/fc.rs` `translate_to_text3_constraints`: the hyphenation
   language came from `-azul-hyphenation-language` alone.

**Fix.**
- Table entries `lang` (order 9) and `xml:lang` (order 10) on any element ->
  `AttributeType::Lang(value.trim())`; the empty value is kept (`lang=""` =
  unknown, hides an ancestor's language). Given both, `xml:lang` lands last,
  and the lookup reads a node's last `Lang` (HTML ranks the XML one first).
- fc.rs: the property where it is set (it stays an override), else the content
  language: `content_language(styled_dom, ifc_root)` walks from the IFC root
  (an anonymous root: its style element) to the nearest `Lang` attribute.
  One reading of a language tag for both, `hyphenation_language_of_tag`
  (replaces the property's inline match): case-insensitive, the full tag first,
  then its primary subtag (`en-AU` -> `en`, RFC 4647 lookup); `en-gb` now maps
  to `EnglishGB` (the other entries as before).
- Fast path: property and `lang` are read only when the IFC's `hyphens` is
  `auto` - the value's only reader is text3's hyphenator (it already ignored
  the language otherwise). A document without `hyphens: auto` reads no
  property and walks no ancestors; with it, one short attribute walk per IFC
  root. (A DOM-wide "has a lang attribute" bit would need a new field on
  `StyledDom` / the compact cache - api.json types - not worth it for this.)

**Tests (RED a8e5b0167).**
- `core/src/xml_attributes.rs` tests: `a_lang_attribute_lands_on_its_node_as_the_content_language`.
- `layout/tests/hyphens_auto_hyphenates_in_the_language_of_the_lang_attribute.rs`:
  a 60px box of "Advertisement" (20px font, 30px lines) compared with the same box
  hyphenated by `-azul-hyphenation-language: en`: `lang` on an ancestor (the repro's
  shape), on the element (`en-US`), the nearest `lang` wins (`EN` inside
  `x-none`), `<html lang="en">`, no hyphenation without `hyphens: auto`, the azul
  property overrides `lang`. Font-independent (heights relative to the reference).

**Files:** core/src/xml_attributes.rs, layout/src/solver3/fc.rs.

## Issue 2 - `position: relative` on an inline span did not move its text

**Root cause.** `positioning::adjust_relative_positions` moves layout BOXES
(`calculated_positions`); the span's own node moved, but the text of an inline
box is painted from its block container's line layout - the glyph runs on
screen, and in a paged list the `TextLayout` payload, which printpdf draws every
shaped glyph from (`get_glyph_runs_pdf` reads each cluster's `position`). Neither
ever learned of the offset. The line layout itself must NOT carry it (CSS 2.2
9.4.3: the shift happens after layout, moves no other box, changes no line;
`ifc_extent` / baselines are computed from item positions), so the shift is
applied where the lines are painted.

**Fix.**
- positioning.rs: `relative_shift(offsets, direction)` - the 9.4.3 rule (top
  over bottom; left over right in ltr, right over left in rtl), pulled out of
  `adjust_relative_positions` unchanged so both users share it.
  `inline_relative_offset(styled_dom, node, ifc_root, cb_size, viewport)`: the
  shifts of every relatively positioned non-atomic inline box enclosing a node,
  added up (nested boxes add), the walk ending at the IFC root (or the first
  non-inline ancestor) - the containing block whose content box the percentages
  and whose `direction` left/right resolve against.
- display_list.rs: `inline_run_shifts` (per glyph run, resolved once per text
  node; `None`, no allocation, when nothing moves); `paint_inline_content`
  places each run's background/border, glyphs, decorations and hit-test area at
  the IFC origin plus its shift (the `::selection` recolour still tests where
  the lines put a glyph; a moved run claims no proven uniform background). In a
  paged list the payload becomes a moved copy (as for `lines_above`) whose
  clusters of moved text nodes carry the shift - built from the dense view's
  items when the stored sparse half is the retirement sentinel. On an
  overflow-visible axis the text clip grows by the shifts.

**Tests (RED 92001bbee)** - `layout/tests/a_relatively_positioned_inline_box_moves_its_text.rs`,
each against the same paragraph without the offset (font-independent):
text + background moved by (left, top); nothing else moves and the paragraph
keeps its size; nested boxes add; `top: -0.45em` = 9px at 20px; the paged
payload's "a" cluster moved by (4, -10), the "J" around it not.

**Files:** layout/src/solver3/positioning.rs, layout/src/solver3/display_list.rs.

**Not covered (follow-ups):** the caret and selection highlight of moved text
stay where the lines put it (`paint_selections` / `paint_cursor` and the
window's text hit-testing read the layout); an atomic inline (inline-block,
image) INSIDE a relatively positioned span is not moved with it; a DL-patching
pass that copies an unchanged IFC's cached runs (`try_copy_cached_run`) would
keep the old shift if only the span's `top` changed without the IFC re-emitting.

## Test commands (parent)

```sh
cargo test -p azul-core --lib a_lang_attribute_lands_on_its_node_as_the_content_language
cargo test -p azul-core --test xml_attributes --test codegen_attributes --test attribute_selectors
# after registering the two files in layout/tests/all.rs:
cargo test -p azul-layout --test all hyphens_auto_hyphenates_in_the_language_of_the_lang_attribute
cargo test -p azul-layout --test all a_relatively_positioned_inline_box_moves_its_text
# regressions around the touched code:
cargo test -p azul-layout --lib positioning
cargo test -p azul-layout --lib display_list
cargo test -p azul-layout --test all
```

## Least sure to compile

- fc.rs `content_language`: `styled_dom.node_data.as_ref()` annotated as
  `&[NodeData]` (so the returned `&str` borrows the DOM, not a local container).
- fc.rs `hyphenation_language_of_tag`: the closure `of` used twice (`of(..)`,
  then `.and_then(of)` inside `or_else`) - a non-capturing closure, so `Copy`.
- display_list.rs: `moved_nodes.iter().find(|(m, _)| *m == n)` (pattern through
  `&&(NodeId, LogicalPosition)`); `dense_view.map(DenseText::to_unified_items)`.
- the paged test's `&mut Some(Vec::new())` for `debug_messages` and
  `Solver3LayoutCache::default()` (Default is derived).

## How pdfocr can verify

Build printpdf against this branch, then `html2pdf repro.zip -o repro.pdf` and
read span positions with PyMuPDF as before:
- page 2 (`page_002.html`): the "a" span's origin y is 0.45em (7.2pt at 16pt)
  above the baseline of "mentioned" / "in Job" (it was equal); x unchanged.
- page 3 (`page_003.html`): the left region (`lang="en"`) breaks "Adver-" /
  "tisement" exactly like the right region (`-azul-hyphenation-language: en`);
  it was one overflowing line. Every page of the book carries `<html lang="en">`,
  so any `hyphens: auto` block now hyphenates without the azul property.

No api.json changes (all new functions are `pub(crate)` / private; no public type
changed).
