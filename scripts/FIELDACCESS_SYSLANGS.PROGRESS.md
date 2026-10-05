# FIELDACCESS_SYSLANGS - progress (field-access wave, 2026-10-05)

Brief: /Users/fschutt/Development/azul-work/field_access_wave.md. Languages: Julia, Nim, Odin, V, OCaml.
Worktree branch reset to bc606e468 (fix/input-bugs-2026-09-19 tip; the worktree was created on old master).
No compiling (parent integrates).

Shared: doc/src/codegen/v2/raw_field_access.rs - classifies each field (Prim / Str / Heap{delete, clone} / Pod;
None = skipped: callbacks, callback wrappers, RefAny, pointers, generics, arrays, heap-without-_delete).

## DONE (all five; nothing compiled - parent integrates)
- Julia: RED 477929b12, fix ce52ca6c9 (raw_field_access.rs + lang_julia/fields.rs).
- Nim: RED 087cdf464, fix 6d4fd100f (lang_nim/fields.rs; tr + azString + `$`).
- Odin: RED 3e13d4768, fix 5e11f82b9 (lang_odin/fields.rs).
- V: RED 947a16ed5, fix 9640c0a2f (lang_v/fields.rs).
- OCaml: RED 857439b04, fix 1c7413cb1 (lang_ocaml/fields.rs, ClassPlan, managed.rs with_layout).
- 088f00cce shared classifier skips opaque field types (Recursive / GenericTemplate / VecRef).

## Case 3 (title + size) per language
- Julia: `opts = Ref(AzWindowCreateOptions_create(cb)); GC.@preserve opts begin ws = window_state_ptr(opts);
  set_title!(ws, "x"); set_dimensions!(size_ptr(ws), AzLogicalSize(400f0, 300f0)) end`
- Nim: `opts.window_state.setTitle("x"); opts.window_state.size.dimensions = AzLogicalSize(width: 400, height: 300)`
- Odin: `azul.FullWindowState_set_title(&opts.window_state, "x"); opts.window_state.size.dimensions = {400, 300}`
- V: `opts.window_state.set_title('x'); opts.window_state.size.dimensions.width = 400`
- OCaml: `WindowCreateOptions.update_window_state opts (fun ws -> FullWindowState.set_title ws "x";
  FullWindowState.update_size ws (fun sz -> WindowSize.set_dimensions sz (LogicalSize.create 400. 300.)))`

## Open questions / not done
- Nim/Odin/V: Prim/Pod fields keep direct field access (safe: no heap); only Str/Heap get procs. Direct
  assignment of a heap field still compiles (and leaks) - documented in the generated header comment.
- Julia `setfields` kept, documented as plain-data only (it cannot release the old value).
- Monomorphized struct aliases (PhysicalSizeU32, ...) get no accessors (all POD; direct access works).
- OCaml raw (non-record) heap fields (Option*/tagged unions with _delete): get returns a clone the caller
  must free (same as methods returning such values); no update_<f> for them.
