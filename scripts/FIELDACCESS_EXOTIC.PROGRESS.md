# Field-access wave - exotic group (Lisp, Racket, Red, Smalltalk, COBOL, ALGOL 68)

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md. No compiling; RED test commit
first per language; commit after every language.

## DONE
- Common Lisp (lang_lisp): RED 4ffdc5bdf, fix (this commit). Wrapped classes' by-value values are
  boxed into a foreign buffer via translate-from-foreign / translate-into-foreign-memory on the
  defcstruct's -tclass (no plists); azul-handle base class + %unwrap/%consume; by-value wrapper args
  are moved; field accessors (<class>-<field> obj) / (setf (<class>-<field> obj) v).

## IN PROGRESS
- Racket (lang_racket).

## NEXT
- Red, Smalltalk, COBOL, ALGOL 68.

## Open questions
(none yet)
