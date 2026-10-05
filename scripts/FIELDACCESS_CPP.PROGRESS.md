# FIELDACCESS_CPP progress (C++ / Zig / C) - field-access wave 2026-10-05

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md
Worktree branch based on fix/input-bugs-2026-09-19 @ bc606e468. No compiling (parent integrates).

## DONE
- 6847ccbec C++ RED test `lang_cpp/mod.rs::field_access_tests` (real api.json, cpp03/11/17/20/23).
- C++ fix (this commit): `common.rs` `cpp_field_accessors` / `generate_field_accessor_decls` /
  `generate_field_accessor_impls` + FIELDS paragraph in `generate_header_comment`; hooked into
  cpp03, cpp11 (cpp14 delegates), cpp17, cpp20 (cpp23 shares). Getter: scalar / std::string
  (C++03: const String deep copy) / `T(Az<T>_clone(&f))` / POD copy; setter: `Az<T>_delete(&f)`
  then `f = value.release()`. C++11+ setters `&`-qualified, C++03 wrapper getters return const.

- debdcbfcd C++ fix.
- 394cf094b Zig RED test (`lang_zig/wrappers.rs` tests, real api.json).
- Zig fix (this commit): `emit_field_accessors` in `lang_zig/wrappers.rs` (getX on `*const Self`
  returns a copy: scalar / `![]u8` dup into an allocator for String / `W{ .inner = C.AzT_clone(&f) }`
  / POD; setX on `*Self` converts via the method-arg `_as*` helper, then `C.AzT_delete(&f)`, then
  stores). `c_decls::primitive` made `pub(super)`. FIELDS docs in the wrapper banner.

- 40f4eb8bc Zig fix.
- ddb4aae91 C RED test (`lang_c.rs::field_helper_tests`, real api.json).
- C fix (this commit): `generate_field_helpers` in lang_c.rs, emitted after the union helpers
  (lang_c already has a `static inline` helper section). Heap-owning fields only:
  `Az<T>_set<Field>(T* instance, F value)` = `Az<F>_delete(&instance->f); instance->f = value;`,
  `Az<T>_get<Field>(const T*)` = `Az<F>_clone(&instance->f)`. Scalars/PODs stay plain C fields.
  Names already exported (incl. Byref/Struct/WithCtx twins) are skipped.

## IN PROGRESS
- (none)

## NEXT
- Parent: `cargo test -p azul-doc field_access` / `field_helper` (the three RED test modules),
  regenerate target/codegen, compile azul03/11/14/17/20/23.hpp + azul.zig + azul.h.

## Open questions
- C++ String getter returns `std::string` (C++11+) but `const String` in C++03 (no <string>).
- Zig String getter takes an allocator (`![]u8`); a wrapper-returning variant was not added.
- C++/Zig/C nested writes are read-modify-write (documented in the header/banner); no views.
