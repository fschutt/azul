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

## IN PROGRESS
- C: static inline helpers for heap-owning fields.

## NEXT
- C: static inline get/set helpers for heap-owning fields (lang_c.rs has a helper section).

## Open questions
- (none)
