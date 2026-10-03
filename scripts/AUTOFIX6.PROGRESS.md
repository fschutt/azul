# AUTOFIX6 progress (wave 6, branch wt/autofix6 from 25d78e309)

Brief: scripts/waves/wave6/AUTOFIX6.md. Files: doc/src/autofix, doc/src/patch (+ minimal wiring in doc/src/main.rs,
noted in the report). Never compile; the parent runs `cargo test -p azul-doc --bin azul-doc -- autofix::` (and `patch::`).

## Plan (one RED test commit, then GREEN, per item)
1. Regression test: a removal targets the module the class is in (178af99a9) - patch_format.rs tests.
2. Gap 1: raw `str` / `&str` / `Optionstr` / `Option<&str>` in a function signature = critical FFI error
   (new check over api.json signatures in mod.rs; any type api.json does not define is UndefinedTypeReference);
   `autofix add` writes `&str` returns as `String` (`azul_css::AzString::from(..)`), `Option<&str>` /
   `Option<String>` as `OptionString` (`.map(|s| azul_css::AzString::from(s)).into()`).
3. Gap 3: std `String` args: MethodArg keeps the type as written (`source_ty`); add converts with
   `{}.into_library_owned_string()`; the signature-drift check reports an existing entry passing it unconverted.
4. Gap 5: ONE wildcard rule (`Type.*`): only methods whose whole signature crosses the FFI as written (no borrowed
   return; every arg / return in its api.json spelling is a scalar, an api.json type or a C-repr workspace type).
5. Gap 2: api.json functions whose Rust method is gone: found by the scan (fn_body `object.m(` / `<path>::m(`, the
   type has no method `m`), reported, removal patches written.
6. Gap 6: no `set_repr` to none (a none means "leave it" -> looped); an unreachable api type whose source has no
   repr is removed; FFI warnings of types removed this round are dropped.
7. Gap 4: pending patches: `autofix add` sees api.json minus the pending removals and supersedes the pending
   removal of the entries it re-adds; patch folders apply in file-name order.
8. `autofix remove module.Type` (whole class): one spec parser for remove and difficult remove.
9. Report scripts/AUTOFIX6_2026_10_03.md.

## Decisions
- Gap 5 rule = the FFI-form rule (no annotation needed; Rust-only helpers that DO have an FFI form stay `pub(crate)`
  per the house rules, or are left out by not using `*`).
- Gap 3 = convert (not only flag): `text.into_library_owned_string()` in the fn_body; `&String` -> `&text.into_..()`,
  `&AzString` -> `&text`.
- Gap 4: an add supersedes a pending function removal of the same entry (drops the name from the pending remove
  patch for the map the add writes into), so the outcome does not depend on the apply order; a pending removal of
  the WHOLE class stops the add with a message (apply first).

## DONE
- 66e6cdadb progress file
- ab372111d item 1: regression test `a_removal_targets_the_module_the_class_is_in` (patch_format.rs tests)
- 409e3c298 item 2 RED: mod.rs `function_signature_tests` (+ RawStrInSignature variant, printer arm, stub);
  function_diff.rs test `a_borrowed_str_return_is_exported_as_an_owned_string` (+ helpers `added`, `returns_of`)
- 48ee23243 item 2 GREEN part 1: `check_function_signatures` in mod.rs (FFI_SCALARS, signature_base_type, is_raw_str)

- item 2 GREEN part 2: scan calls check_function_signatures; convert_return_type_for_ffi -> ReturnConversion
  (commits "the scan runs the function-signature check", "add exports a borrowed str as String ...")

## IN PROGRESS

- item 3 RED + GREEN (MethodArg.source_ty + written_type_name in type_index.rs; source_arg_ffi_type string rules;
  passes_bare drift report in find_function_differences)

- item 4 RED + GREEN: api_candidate_methods(.., carries); ffi_carries; wildcard_skip_reason; main.rs 2 call sites

- item 5 RED + GREEN + scan integration (mod.rs: `removed_classes`, `gone`, patches `NNNN_remove_fns_<Class>`)

## NEXT (exact)
- item 6 (gap 6) RED: diff.rs test module `repr_loss_tests`: `a_type_that_lost_its_c_repr_gets_no_repr_patch`
  (compare_derives_and_impls) and `an_unreachable_type_without_a_c_repr_is_removed` (generate_diff_v2 with
  ResolvedTypeSet::default(), index via add_type_for_test). GREEN: in compare_derives_and_impls skip ReprChanged
  when workspace repr is None; `still_exposed_in_source(index, name)` (Some def and, for struct/enum, repr.is_some())
  replaces `index.resolve(..).is_some()/is_none()` at the 3 removal guards; at the end drop modifications /
  additions / path_fixes / module_moves of every removed type; mod.rs: drop FFI warnings of removed classes
  (reuse `removed_classes` - move its computation up before the ffi checks).
- then items 7, 8, 9 as in the plan above.
