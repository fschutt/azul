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

- Smalltalk (lang_smalltalk): RED f4539f54f, fix (this commit). `fieldsDesc` (`^ #( type name; ...)`,
  padding/arrays spelled out as uint8/elem fields, callback-typedef fields void*) + class-side
  `initialize [ self compileFields ]`; bug_classes smalltalk record/padding spelling updated.
  Wrappers: azulConsume; by-value wrapper args go through `AzulNative azulMove:` (fixes
  AzulApp>>run: double free), pointer args `azulBorrow:`; field accessors `<field>` (String ->
  Smalltalk String, heap value -> wrapped _clone, else UFFI copy) and `<field>:` (_delete then move).

## IN PROGRESS
- COBOL (lang_cobol).

## NEXT
- ALGOL 68.

## Open questions
(none yet)
