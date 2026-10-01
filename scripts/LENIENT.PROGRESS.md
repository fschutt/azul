# LENIENT - progress (branch wt/lenient, base 16d19442c)

Brief: scratchpad/LENIENT_go.md (lenient XML/HTML, components audit, paste parser).

## DONE
- `3012f7adf` wip: core/src/xml_html.rs + xml_entities.rs + scripts/gen_html_entities.py (unwired)
- `005227b7a` test data: tests/mail_corpus/outlines (Chrome's trees) + scripts/refci/mail_outlines.py
- `b70c93605` RED: core xml_html_test.rs, layout real_mail_html_parses_like_a_browser.rs,
  the_two_xml_loaders_build_one_tree.rs; stubs in core xml_html.rs + layout parse_html*

## IN PROGRESS
- Design: one tree construction (`azul_core::xml::html`) for every loader; a lenient
  HTML tokenizer in core; the strict loaders keep xmlparser as their tokenizer.
- Drafted (uncommitted, kept until the RED commit lands): core/src/xml_html.rs (type-checked
  and clippy-clean in a stub harness), core/src/xml_entities.rs + scripts/gen_html_entities.py.
  A line-by-line Python mirror (scratchpad/lenient/mirror.py) builds Chrome's tree for 16/18
  corpus mails (the 2 others: foster parenting, a documented simplification) and 43/50 snippets
  (the 7 others: comment nodes, control characters dropped on purpose, simplified adoption).
- The layout splice (both strict loaders onto the builder, FastDomSink, feed_xml_tokens) is
  parked at scratchpad/lenient/mod.rs.new (made by scratchpad/lenient/splice_loaders.py from the
  base file); layout/src/xml/mod.rs is back at base for the RED commit.
- NEXT STEP (GREEN 1): copy scratchpad/lenient/xml_html.full.rs over core/src/xml_html.rs
  (it now implies <html>/<body> for fragments and has the `mod tests` decl); declare
  `mod entities` in xml.rs; html_named_entity on the new table. Then GREEN 2: layout from
  scratchpad/lenient/mod.rs.new (+ the lenient pub fns replacing the RED stubs,
  hide <style> nodes in FastDomSink, element_draws_nothing pub, decode_xml_entities on core,
  the `&LT;` test row); GREEN 3: core DOM builders read names lower-cased, foreign elements
  dropped only inside svg, tag_to_node_type: a foreign prefixed tag is a Span.

## NEXT
1. RED: lenient parse of the mail corpus + E-XML-3 (the two loaders agree).
2. FIX: core/src/xml_html.rs (tokenizer, tree builder, XmlNode sink) + layout adapters.
3. Entities: the full HTML5 table.
4. Components audit (ol/li/ul/img/font/center/align/body), list-style-position: inside.
5. Paste parser on the lenient loader (Word / browser paste).
6. Report scripts/LENIENT_2026_10_01.md.

## Decisions
- xmlparser is RazrFalcon's crates.io crate (not ours, a [patch] trips the release gate):
  the lenient tokenizer is written in azul-core instead (brief: "prefer fixing our own layer").
- Type-checking: `rustc --emit=metadata` on a standalone harness of the self-contained core
  module (the analogue of the allowed `clang -fsyntax-only`); nothing is linked or run.

## Open questions
- (none)
