# X1b - DOM export in 15 printers (progress checkpoint)

Branch `wt/x1b-dom-export-printers` (from 0a326afe5). Languages: ada, algol68, cobol, crystal,
fortran, freebasic, lisp, odin, perl, powershell, racket, red, smalltalk, v, vb6.
Extra scope (coordinator): racket.rs builds padded union variant records positionally; make it
set them by field name (F1 pads every variant in the bindings).

## DONE
- fe0ac6551 test(css): racket sets a union variant by field name (RED)
- 73d3b6e4e fix(css): racket sets a union variant by field name
- d8da7de4f test(css): crystal and odin export the DOM (RED: hand-written dom_card goldens)
- 149adf4a5 feat(css): crystal and odin export the DOM (functions, params, concat, app,
  registration)
- 5560953f5 test(css): ten more printers export the DOM (RED, structure-test list)
- f21ac11a2 feat(css): linear.rs DOM hooks; v, ada, fortran, freebasic
- fbb4abd56 feat(css): lisp, racket, powershell, smalltalk
- bc23d95ba fix(css): perl, red, vb6, cobol, algol68 say why (precise dom_limitation)

- 8538420dd test(css): drop the stale dom_app goldens
- 248cd159e fix(css): Ada / FreeBASIC apps import AzWindowCreateOptions_create as exported
- 1207452de fix(css): red / vb6 limitations accurate after F1
- report: scripts/X1B_DOM_EXPORT_PRINTERS_2026_09_29.md

## IN PROGRESS
(none)

## NEXT
- parent: compile, bless goldens (report section 8)

## Open questions
- X1a adds the same `is_dom_item` / `one_line` / `registration_note` to dom.rs and the same
  `call_param_names` hunk to mod.rs: on merge keep one copy (text is identical).
- Syntax check without cargo: `rustfmt --edition 2021 --check <file>` (only parse errors matter;
  the formatting diffs come from the nightly-only rustfmt.toml).
