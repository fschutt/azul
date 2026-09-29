# F1 - bindings layout follow-ups (progress checkpoint)

Branch `wt/f1-bindings-layout`, cut from 282890483 (local tip of `fix/input-bugs-2026-09-19`).
Nothing is compiled here (house rule). If resumed: read this file first.

## DONE

(none yet)

## IN PROGRESS

- P0 RED: exhaustive test that every tagged-union variant payload in azul.h sits where Rust's
  `repr(C, u8)` / `repr(C)` puts it.

## NEXT

1. P0 fix: one function in `c_layout.rs` (payload offset + per-variant padding), lang_c emits the
   padding + `_Static_assert`s, c_layout models the padded header.
2. P0 Rust ground truth: `css/tests/a_union_payload_sits_after_the_largest_alignment.rs`.
3. P0 every mirroring binding emits the padding from the same function.
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
