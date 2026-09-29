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

## IN PROGRESS

- P0 bindings: 3 read-only Explore agents survey every binding's union layout emission
  (groups: go/pascal/node/crystal/odin/v/racket; java/kotlin/csharp/d/zig/nim/julia/
  smalltalk/swift; ada/algol68/cobol/freebasic/haskell/lisp/ocaml/perl/red/ruby/vb6/fortran/
  python/lua/php). Then edit each binding to ask c_layout for the padding.

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
