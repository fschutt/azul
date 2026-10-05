# FIELDACCESS_SYSLANGS - progress (field-access wave, 2026-10-05)

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md. Languages: Julia, Nim, Odin, V, OCaml.
Worktree branch reset to bc606e468 (fix/input-bugs-2026-09-19 tip; the worktree was created on old master).
No compiling (parent integrates).

Shared: doc/src/codegen/v2/raw_field_access.rs - classifies each field (Prim / Str / Heap{delete, clone} / Pod;
None = skipped: callbacks, callback wrappers, RefAny, pointers, generics, arrays, heap-without-_delete).

## DONE
(none yet)

## IN PROGRESS
- Julia: RED test (lang_julia/mod.rs field_access_tests).

## NEXT
- Julia fix, Nim (+ tr template, "" IndexDefect), Odin, V, OCaml (+ with_layout leak).

## Open questions
