# TEXT7 - text layout bugs (wave 7)
Owns: layout/src/text3/*, layout/src/font* (loading, parsed metrics), and ONLY line ~1555 of
dll/src/desktop/shell2/common/layout.rs. Read first: scripts/MAILENG6_2026_10_03.md, scripts/TEXTENG_2026_10_02.md.

1. TEXT-INDENT DOES NOT NARROW THE FIRST LINE (top priority; it blocks the pdfocr project). Report from the pdfocr
   agent (azul at 2e92c759b): `<p style="width:600px;font-size:16px;text-align:justify;text-indent:120px">` (80
   words) - the first line starts at 120 px (correct) but is broken against the FULL width, so it ends ~716 px
   (justify) / ~684 px (left); every other line ends by ~595 px. Root cause (layout/src/text3/cache.rs): the
   greedy path perform_fragment_layout -> get_line_constraints (~:11237) -> break_one_line (~:11606) breaks
   against total_available and never subtracts constraints.text_indent for the FIRST line of the paragraph;
   position_one_line then adds the indent to the pen (~:12377-12391) AFTER the justify spacing was computed for
   the full segment width (~:12666-12810). Knuth-Plass does it right (knuth_plass.rs:398-405, first_line_width =
   line_width - text_indent) but only runs for text-wrap: balance (cache.rs ~:10945-10964). The first line's
   available width must be width - indent on the greedy path too, and justify must spread over that width.
   Negative indents (hanging) and the indent only on the paragraph's first line (not after a forced break /
   not on a continuation fragment) - check CSS Text 3 s8.1. READY RED TESTS: /Users/fschutt/Development/pdfocr
   results/azul-text-indent-repro/a_text_indent_narrows_the_first_line.rs (left, justify, an indent in pt) -
   copy it into layout/tests/ (register in all.rs); if that path does not exist, search ~/Development for
   a_text_indent_narrows_the_first_line.rs, else write the three tests from the repro above. The existing
   the_first_line_of_a_paragraph_starts_text_indent_further_in.rs must keep passing.
2. A text run shaped before its font has loaded stays invisible after the font arrives (MAILENG6 report): the run
   must be reshaped / its cache key must include the loaded state.
3. font-weight: bolder / lighter are fixed weights; they must be relative to the parent's computed weight (CSS
   Fonts 4 s2.2 table).
4. The font-index off-by-one MAILENG6 fixed in layout also exists at dll/src/desktop/shell2/common/layout.rs:1555
   (a text node keyed on the font of the node BEFORE it) - same fix, a test where the dll path is reachable.
5. line-height: 19px gives a 19.55 px pitch (0.55 too large; found probing pdfocr markup) - RED with a 19px
   paragraph of 5 lines: baselines 19 px apart.
6. line-height in rem / vw / vh is rejected (StyleLineHeight cannot carry the context) - extend it (TEXTENG
   added the StyleLineHeight enum) and resolve against the root font size / viewport.

Rules: scripts/waves/house_rules.md (read it fully first). Plan + who owns what: scripts/waves/wave7/PLAN.md.
Every behaviour change: a RED test commit first (test names are sentences), then the fix. Root causes, no
workarounds. Never compile. Commit after every unit; keep scripts/TEXT7.PROGRESS.md exact. Finish with the report
scripts/TEXT7_<YYYY_MM_DD>.md (built, commits, api.json list in api.json terms, least-sure-to-compile spots, test
commands, what is left) and commit it.
