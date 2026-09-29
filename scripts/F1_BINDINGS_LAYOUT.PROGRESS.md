# F1 - bindings layout follow-ups (progress checkpoint)

Branch `wt/f1-bindings-layout`, cut from 282890483 (local tip of `fix/input-bugs-2026-09-19`).
The PR tip has since moved to 0a326afe5: do NOT merge it (the coordinator cherry-picks).
Nothing is compiled here (house rule). If resumed: read this file first.

## DONE

P0 - tagged-union payload offset:
- 5af803035 test RED: exhaustive union payload offsets (doc bug_classes) + Rust ground truth
  `css/tests/a_union_payload_sits_after_the_largest_alignment.rs`.
- 9ca29759c fix: `c_layout::union_payload_layout` / `variant_payload_padding` (the one place),
  lang_c's two union emitters merged into `generate_union_type`, `_pad0[N]` +
  `AZ_LAYOUT_CHECK(offsetof(..) == N, ..)` blocks, c_layout models the padded structs, doc fixed.
  Hand-checked on a scratch copy of azul.h with clang (C99/C11/C17/C++03/11/20 all pass).
- cf13151b1 test RED + 8f1b6dd45 fix: every binding with per-variant records pads like azul.h
  (15 bindings), OCaml extractor uses payload_offset and its own layout calculator is gone,
  VB6 comment. Hand-checked go/zig/odin/ldc2/nim/crystal/v/racket snippets: payload at 8.
- 9a8038c93 test RED + a3aa9e8d5 feat: conformance C program reads every comparable variant
  payload back through the union (`VariantCase::payload_check`).

P1:
- fd2ce2478 test RED + 7b2dd2fdd fix: azul.h compiles as C (AzString_fromStaticBytes /
  AzString_trStaticBytes; bug class: no name is macro+function or static+extern).
- e865f4590 fix(zig): comptime String.tr keyed on TypeCategory::String (`handwritten` set).
- 2f44856a5 fix(core): `from_handle_byref_fn` mandatory in impl_managed_callback! (5 forms);
  tree_view.rs, video.rs, host_invoker_test.rs name theirs.
- b7d1f01ef fix(d): `&mut self` of a native type = `ref` receiver + write back
  (`stringLocalizable(ref string self, bool)`).

P2:
- ca4792584 refactor: GetHash imports -> `azul_css::hash::GetHash`; layout drops `codegen`.
- 5aa26ede8 test RED + ec0609854 fix(ada): C_Pass_By_Copy records, `'Size use 8` u8 tags.
- a5b8b85b7 test RED + 3dab8a0d3 fix(freebasic): integer widths, sort_order emission, aliases,
  VecRef/destructor types emitted, UByte u8 tag.
- 076ffec91 test RED + 87100fd51 fix(red): unions are C-sized blobs, sort_order, mono aliases.

## IN PROGRESS

- P2 VB6: Declare the `<fn>Byref` twins libazul exports (by-pointer aggregates), instead of
  SKIPPED Declares that pass UDTs ByRef to by-value C functions.

## NEXT

1. P2 ALGOL 68: list precisely (a68g 3.11.3 installed: `--check` on the types section shows
   `PROC (REF VOID)` etc. rejected; `ALIEN` undeclared).
2. Final report `scripts/F1_BINDINGS_LAYOUT_2026_09_29.md`.

## Facts found

- Current azul.h (target/codegen, 2026-09-29 08:00), measured with
  `clang -Xclang -fdump-record-layouts-simple`: 47 tagged unions / 210 variants have a C
  payload offset unlike Rust's (all `repr(C, u8)`; CssProperty: 97 variants at 1 or 4 instead
  of 8; StyleBackgroundContent::Color at 1 instead of 8). Union SIZES agree everywhere.
- api.json: every data-carrying enum is `repr(C, u8)`, every unit enum `repr(C)`; every
  variant is a unit or a one-element tuple (no struct variants, no multi-field tuples).
- Survey: pascal/freebasic/ruby/ada already put payloads at the largest alignment (variant
  records / tag + union); c++/swift/lua/php/haskell follow azul.h; fortran/cobol/perl/red/vb6/
  algol68/python never read a payload at an offset.

## Open questions

(none)
