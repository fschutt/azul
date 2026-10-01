# LENIENT - progress (branch wt/lenient, base 16d19442c)

Brief: scratchpad/LENIENT_go.md (lenient XML/HTML, components audit, paste parser).

## DONE
- (none yet)

## IN PROGRESS
- Design: one tree construction (`azul_core::xml::html`) for every loader; a lenient
  HTML tokenizer in core; the strict loaders keep xmlparser as their tokenizer.

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
