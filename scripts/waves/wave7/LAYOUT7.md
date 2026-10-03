# LAYOUT7 - block / inline / table / sizing bugs (wave 7)
Owns: layout/src/solver3/* EXCEPT display_list.rs (PAINT7) and page_breaks.rs (another session - never touch);
css/src/props for the new `zoom` property. Read first: scripts/MAILENG6_2026_10_03.md, scripts/MAIL6_2026_10_03.md
(engine bugs), scripts/WRITER6_2026_10_03.md sec. 6, scripts/TABLES_2026_10_02.md.

1. An absolutely positioned child is treated as IN-FLOW: in AzNotes' check item (`li`, display:list-item,
   list-style-type:none, position:relative; children: text, then an abspos div) the li becomes a block formatting
   context with a ::marker (generated although list-style-type is none), an anonymous inline wrapper and the
   abspos box as an in-flow child -> every check item is drawn twice (AzNotes, AzWriter, AzMail compose).
   CSS 2.2 s9.2.1.1: only in-flow block-level children split a block container's inline content; s9.7.
   RED (WRITER6): an_absolutely_positioned_child_does_not_split_its_parents_line, plus no marker box when
   list-style-type is none.
2. A block inside an inline (`<a><img style="display:block"></a>`) is dropped from layout (MAILENG6) - CSS 2.2
   s9.2.1.1 block-in-inline: the inline is split around an anonymous block.
3. inline-block with min-width:100% + box-sizing:border-box + padding measures 524 px in a 500 px container, not
   500 (MAIL6).
4. width: fit-content acts like 100% (MAIL6) - min(max-content, max(min-content, available)).
5. display: table drops a child <p>'s margins (MAIL6).
6. CSS `zoom` (MAIL6: AzMail's zoom only scales text without its own px sizes): the property (css parse +
   cascade, not inherited, multiplies) and its effect on used lengths (CSS Viewport / the zoom spec Chrome
   follows); AzMail then uses it on the reading pane (note it for OFFICE7, do not edit the app).
7. AzMail's account wizard stops at page 2 - an engine bug with a RED test already committed (cc7040ae5): find it
   (`git show cc7040ae5`), root-cause, fix.
8. An inline-block inside an inline span is sized from its max-content width, ignoring its CSS width
   (fc.rs collect_inline_span_recursive, the LayoutDisplay::InlineBlock arm).
9. Restore the real assertion of a_narrow_table_wraps_its_cells_to_fit (407cc8c98 loosened >=3 to >=2): each
   cell exactly 2 baselines, widths per CSS Tables 3 s9.3.3 min/max split; 56b105f60 dodged an inline-block
   min-content bug with no RED test - find and fix it.
10. A block taller than one page overflows its sheet (AzWriter pagination): find where the paginator
   (NOT page_breaks.rs) places an unbreakable/too-tall block; per CSS Fragmentation, a block taller than the
   fragmentainer is split across pages (or, if monolithic, overflows - say which applies and test it). If the
   fix needs page_breaks.rs, stop at the RED test and report it.

Rules: scripts/waves/house_rules.md (read it fully first). Plan + who owns what: scripts/waves/wave7/PLAN.md.
Every behaviour change: a RED test commit first (test names are sentences), then the fix. Root causes, no
workarounds. Never compile. Commit after every unit; keep scripts/LAYOUT7.PROGRESS.md exact. Finish with the report
scripts/LAYOUT7_<YYYY_MM_DD>.md (built, commits, api.json list in api.json terms, least-sure-to-compile spots, test
commands, what is left) and commit it.
