# FIELDACCESS_SYSLANGS - progress (field-access wave, 2026-10-05)

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md. Languages: Julia, Nim, Odin, V, OCaml.
Worktree branch reset to bc606e468 (fix/input-bugs-2026-09-19 tip; the worktree was created on old master).
No compiling (parent integrates).

Shared: doc/src/codegen/v2/raw_field_access.rs - classifies each field (Prim / Str / Heap{delete, clone} / Pod;
None = skipped: callbacks, callback wrappers, RefAny, pointers, generics, arrays, heap-without-_delete).

## DONE
- Julia: RED 477929b12; fix = next commit (raw_field_access.rs + lang_julia/fields.rs; generic
  `get_<f>(x)` / `set_<f>!(x, v)` / `<f>_ptr(x)` on `Ref{AzT}` or `Ptr{AzT}` views; setfields doc'd as
  plain-data only; examples/julia/hello-world.jl uses the accessors).

- Julia fix ce52ca6c9.
- Nim: RED 3a-commit before the fix; fix = lang_nim/fields.rs: `azString` (empty-safe, copies), `$` (copies,
  never consumes), `tr` = `AzString_tr(azString(key))` (no borrowed GC buffer, no `key[0]` on ""), and
  `get<F>` / `set<F>` for Str/Heap fields only (Prim/Pod stay direct fields). A name an api.json method
  owns for the same receiver (ProcDedup::has_receiver) falls back to `get<F>Field` / `set<F>Field`.

## IN PROGRESS
- Odin.

## NEXT
- Odin, V, OCaml (+ with_layout leak).

## Open questions
