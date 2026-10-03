# LENIENT - progress (branch wt/lenient, base 16d19442c)

Brief: scratchpad/LENIENT_go.md (lenient XML/HTML, components audit, paste parser).

## DONE
- `3012f7adf` wip: core/src/xml_html.rs + xml_entities.rs + scripts/gen_html_entities.py (unwired)
- `005227b7a` test data: tests/mail_corpus/outlines (Chrome's trees) + scripts/refci/mail_outlines.py
- `b70c93605` RED: core xml_html_test.rs, layout real_mail_html_parses_like_a_browser.rs,
  the_two_xml_loaders_build_one_tree.rs; stubs in core xml_html.rs + layout parse_html*
- `946585e03` GREEN 1: core lenient loader + full entity table
- `bc27da327` GREEN 2: layout strict loaders on the shared builder, FastDomSink, lenient API
- `7280ef503` GREEN 3: core DOM builders lower-case names, foreign elements dropped only in svg, o:p = Span
- `8a9e682d7` builder.rs twins (VOID_ELEMENTS / AUTO_CLOSE) -> core is_void_element / start_tag_closes
- `86aa95f82` RED paste / `f35506a22` GREEN paste (lenient tree, o:p inline, Word mso-list lists)
- `a69f5195f` RED components / `e6045f05f` GREEN presentational hints / `cb1f1eead` GREEN counters
- `af63953e4` RED / `12a06e54b` GREEN list-style shorthand; `d74733a8b` transient-window in the tree loader
- report scripts/LENIENT_2026_10_01.md (`ae11f8a73`, `225cecae5`, `739aa8ca8`)

## IN PROGRESS
- (none) - TASK COMPLETE; the report lists what is left.

## NEXT
- Parent: compile, run the test commands in the report, add the api.json entries
  (Xml::create_from_html, CombinedCssPropertyType::ListStyle).

## Decisions
- xmlparser is RazrFalcon's crates.io crate (not ours, a [patch] trips the release gate):
  the lenient tokenizer is written in azul-core instead (brief: "prefer fixing our own layer").
- Type-checking: `rustc --emit=metadata` on a standalone harness of the self-contained core
  module (the analogue of the allowed `clang -fsyntax-only`); nothing is linked or run.

## Open questions
- (none)
