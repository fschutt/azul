# X1b - DOM export in 15 printers (progress checkpoint)

Branch `wt/x1b-dom-export-printers` (from 0a326afe5). Languages: ada, algol68, cobol, crystal,
fortran, freebasic, lisp, odin, perl, powershell, racket, red, smalltalk, v, vb6.
Extra scope (coordinator): racket.rs builds padded union variant records positionally; make it
set them by field name (F1 pads every variant in the bindings).

## DONE
(none yet)

## IN PROGRESS
- racket: union variants by field name (RED goldens, then fix)

## NEXT
- RED: crystal + odin `dom_card` goldens by hand, structure test list
- expression printers: crystal, odin, lisp, racket, perl, powershell, smalltalk
- linear printers: shared DOM hooks in lang/linear.rs, then v, ada, fortran, freebasic
- limitations: algol68, cobol, red, vb6 (precise reasons)

## Open questions
- X1a also needs DOM in a linear printer (pascal): the linear.rs hooks are shared.
