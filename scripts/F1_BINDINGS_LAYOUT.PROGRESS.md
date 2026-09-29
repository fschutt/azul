# F1 - bindings layout follow-ups (progress checkpoint)

Branch `wt/f1-bindings-layout`, cut from 282890483 (local tip of `fix/input-bugs-2026-09-19`).
Nothing is compiled here (house rule). If resumed: read this file first.

## DONE

- 5af803035 test(codegen): RED - exhaustive union payload offsets (doc bug_classes) + Rust
  ground truth `css/tests/a_union_payload_sits_after_the_largest_alignment.rs`.
- 9ca29759c fix(codegen): `c_layout::union_payload_layout` / `variant_payload_padding` (the one
  place), lang_c's two union emitters merged into `generate_union_type`, `_pad0[N]` +
  `AZ_LAYOUT_CHECK(offsetof(..) == N, ..)` blocks, c_layout models the padded structs, doc fixed.
  Hand-checked on a scratch copy of azul.h with clang (C99/C11/C17/C++03/11/20 all pass).

- fd2ce2478 test RED + 7b2dd2fdd fix: azul.h compiles as C (AzString_fromStaticBytes /
  AzString_trStaticBytes; bug class no name is macro+function or static+extern).
- e865f4590 fix(zig): comptime String.tr keyed on TypeCategory::String (`handwritten` set).
- 2f44856a5 fix(core): `from_handle_byref_fn` mandatory in impl_managed_callback! (5 forms);
  tree_view.rs, video.rs, host_invoker_test.rs name theirs.
- b7d1f01ef fix(d): `&mut self` of a native type = `ref` receiver + write back
  (`stringLocalizable(ref string self, bool)`).

- cf13151b1 test RED + 8f1b6dd45 fix: every binding with per-variant records pads like azul.h
  (15 bindings), OCaml extractor uses payload_offset and its own layout calculator is gone,
  VB6 comment. Hand-checked go/zig/odin/ldc2/nim/crystal/v/racket snippets: payload at 8.

## IN PROGRESS

- P0 conformance case per constructible union variant (if cheap).

## Survey notes (P0 bindings)

- Survey (3 Explore agents) result:
  - NEED `_pad0[N]` (C-aligned records, from c_layout padding): go, node/koffi (Deno resolve()
    cannot parse arrays -> emit N uint8_t members or teach resolve), crystal, odin, v, racket
    (define-cstruct positional make-* arity changes), java (+@FieldOrder), kotlin
    (+getFieldOrder), csharp (N byte fields, no arrays), d, zig (thread ir), nim (leading `_`
    maybe illegal), julia, smalltalk (tag typed as FFIExternalEnumeration, width?), lisp.
  - NEED the offset itself: ocaml (Option/Result extractor uses own alignment,
    lang_ocaml/types.rs:571-578).
  - ALREADY RIGHT (tag + union of tag-less payloads / variant records): pascal, freebasic, ruby,
    ada (probably; GNAT variant part). Do not pad them.
  - AUTOMATIC via azul.h: c++, swift, lua, php, haskell (cshim offsetof oracle).
  - OPAQUE/unaffected: fortran, cobol, perl, red, vb6 (comment says payload at 4: doc fix),
    algol68, python.

## NEXT

3. P0 every mirroring binding emits the padding from the same function (+ lint test that every
   emitter declaring per-variant records calls c_layout).
4. P0 conformance case per constructible union variant (if cheap).
5. P1 three failing bug-class tests (zig "String", byref exports of two callbacks, D setLocalizable).
6. P1 azul.h C compile errors (AzString_fromConstStr macro + fn, AzString_tr linkage) + bug class.
7. P2 Ada / algol68 / freebasic / red / vb6 notes from B2.
8. P2 GetHash imports -> `azul_css::hash::GetHash`.

## Facts found

- Current azul.h (target/codegen, 2026-09-29 08:00), measured with
  `clang -Xclang -fdump-record-layouts-simple`: 47 tagged unions have at least one variant whose
  C payload offset differs from Rust's (all `repr(C, u8)`; e.g. CssProperty: 96 variants at 4
  instead of 8; StyleBackgroundContent::Color at 1 instead of 8). Union SIZES agree everywhere.
- api.json: every data-carrying enum is `repr(C, u8)`, every unit enum `repr(C)`; every
  variant is a unit or a one-element tuple (no struct variants, no multi-field tuples).

## Open questions

(none yet)
