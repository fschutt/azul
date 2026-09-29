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

## IN PROGRESS
- v (C-like, linear printer): needs DOM hooks in lang/linear.rs

## NEXT
- linear printers: shared DOM hooks in lang/linear.rs, then v, ada, fortran, freebasic
- expression printers: lisp, racket, perl, powershell, smalltalk
- limitations: algol68, cobol, red, vb6 (precise reasons)
- final: structure-test list, report

## Open questions
- X1a also needs DOM in a linear printer (pascal): the linear.rs hooks are shared.
- Syntax check without cargo: `rustfmt --edition 2021 --check <file>` (only parse errors matter;
  the formatting diffs come from the nightly-only rustfmt.toml).
