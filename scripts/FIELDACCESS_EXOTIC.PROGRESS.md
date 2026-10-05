# Field-access wave - exotic group (Lisp, Racket, Red, Smalltalk, COBOL, ALGOL 68)

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md. No compiling; RED test commit
first per language; commit after every language.

## DONE
- Common Lisp (lang_lisp): RED 4ffdc5bdf, fix (this commit). Wrapped classes' by-value values are
  boxed into a foreign buffer via translate-from-foreign / translate-into-foreign-memory on the
  defcstruct's -tclass (no plists); azul-handle base class + %unwrap/%consume; by-value wrapper args
  are moved; field accessors (<class>-<field> obj) / (setf (<class>-<field> obj) v).

- Racket (lang_racket): RED bcb43f03d (+ c_layout::field_offsets), fix (this commit). azul-string->string
  reads ptr/len at IR-derived offsets and memcpys into make-bytes (no make-sized-byte-string);
  resource-owning structs get (<class>-<field> obj) [String -> Racket string, else define-cstruct
  view], (<class>-<field>-copy obj) [_clone], (set-<class>-<field>! obj v) [_delete at
  (ptr-add obj <azul.h offset>) then store]. Example uses set-full-window-state-title!.

- Red (lang_red): RED ac07a3981, fix (this commit). Struct fields: bool -> byte!, i16/u16 -> two
  byte! (name, name_1), i64/u64 -> two integer! (name, name_hi), explicit _pN [byte!] padding to
  the azul.h offsets and size (c_layout::field_offsets / type_layout). Argument/return mapping
  unchanged (bool args/returns are still logic!).

## IN PROGRESS
- Smalltalk (lang_smalltalk).

## NEXT
- COBOL, ALGOL 68.

## Open questions
(none yet)
