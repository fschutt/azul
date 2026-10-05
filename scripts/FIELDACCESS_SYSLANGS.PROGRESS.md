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
- Odin: RED then fix (lang_odin/fields.rs): `az_string` / `az_string_to_odin` helpers;
  `<Class>_get_<f>(&x)` / `<Class>_set_<f>(&x, v)` for Str/Heap fields (pointer receiver = nested writes).
- Nim: RED 087cdf464, fix 6d4fd100f = lang_nim/fields.rs: `azString` (empty-safe, copies), `$` (copies,
  never consumes), `tr` = `AzString_tr(azString(key))` (no borrowed GC buffer, no `key[0]` on ""), and
  `get<F>` / `set<F>` for Str/Heap fields only (Prim/Pod stay direct fields). A name an api.json method
  owns for the same receiver (ProcDedup::has_receiver) falls back to `get<F>Field` / `set<F>Field`.

- V: RED then fix (lang_v/fields.rs): `az_string_to_v(&s)` (copies, never consumes);
  methods `x.get_<f>()` / `x.set_<f>(v)` for Str/Heap fields (`mut` receiver = nested writes).

- OCaml: RED then fix (lang_ocaml/fields.rs + ClassPlan): `get_<f>` / `set_<f>` / `update_<f>` in every
  class module (`.mli` + `.ml`); records consumed on set (`disposed <- true`), deep-copied on get
  (`make_<r> (<clone> ptr)`); `update_<f>` = working copy written back via Fun.protect (getf copies, so
  nested writes need it). `azul_string_of_az` (non-consuming decode) in azul_managed.
  `azul_<class>_with_layout` now calls `Az<LayoutCallback>_delete` on the default callback before setf.
  Example examples/ocaml/hello_world.ml (included into the generated tree) sets title + size.

## IN PROGRESS
- (none) - final review.

## NEXT
- Parent: cargo test -p azul-doc (field_access_tests in 5 langs + raw_field_access tests), regenerate,
  compile the 5 hello-worlds.

## Open questions
