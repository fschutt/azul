# TOOLS7 progress (branch wt/tools7, base 2e55eef06)

## DONE
- f6b29c26e progress file
- item 0 (one method per new type):
  - f07526e27 RED test autofix::add::tests::two_add_patches_for_one_new_type_keep_both_methods
    (new module doc/src/autofix/add.rs; main.rs new-type path calls it; AddTypeResult.functions_patch)
  - f044740e7 GREEN: file names say what they hold (add_<t>.type.<x>, add_<t>.impls, add_<t>_<spec>)
  - 9a9a744c5 RED patch::tests::the_apply_reports_a_function_a_later_patch_dropped
  - ddf300767 GREEN patch::dropped_entries (patch/mod.rs, outside the owned list - minimal)
- item 1 (`autofix add Class.name --fn <path>`):
  - dce36deda RED type_index::tests::a_free_function_is_found_through_a_re_export_or_a_path_module
  - d36edd384 RED function_diff::tests::{a_free_function_becomes_a_function_of_the_class,
    a_callback_info_argument_crosses_by_value_and_is_re_borrowed}
  - 74ec2c71f GREEN find_free_fn (#[path] modules, re-exporting parents); CRATE_DIRS const
  - a88f2ce23 GREEN free_fn_entry + by-value CallbackInfo + function_data_for_call
  - cd2b046b9 feat: main.rs `--fn` arm + multi-spec `autofix add T.a T.b`; generate_add_entries_patch
  - eafac9034 test add::tests::an_add_with_fn_writes_the_entry_into_the_class
- item 2 (bare `object`):
  - 25c3f85ba RED mod.rs function_signature_tests::a_fn_body_passing_a_bare_object_receiver_is_a_critical_error
  - e5de1ad90 GREEN check_fn_body_receivers + BareObjectInFnBody (critical)
  - 89c1fc76c RED function_diff::tests::a_destroy_method_is_called_and_no_body_passes_a_bare_object
  - a3d68900c GREEN destroy* bodies call the method (generate_destructor_body gone)
- item 3 (module choice, private paths):
  - ff9d487db RED module_map::tests::a_new_type_goes_where_the_scan_keeps_it
  - 5c2d874a6 GREEN new_type_module (one rule: add, deps, scan additions, Add-op fallback);
    is_structural helper; cpurender -> image arm; table entry TextRasterStyle -> image
  - 3c1d0d238 AzPhoto import azul::image::TextRasterStyle (depends on the scan's move!)
  - e09122405 RED type_index::tests::a_type_in_a_private_module_is_indexed_by_its_public_re_export_path
  - 2d2e48396 GREEN ModuleFacts / public_path / private_modules / private_module_on
  - c11f9e711 RED diff path_fix_tests::a_path_through_a_private_module_is_fixed_to_the_public_re_export,
    mod.rs ..::a_class_behind_a_private_module_nothing_re_exports_is_a_critical_error
  - 1b4de30bd GREEN needs_path_fix(index) + check_private_paths + PrivateExternalPath
  - 1a1d8ef44 RED add::tests::an_add_of_a_type_behind_a_private_module_is_refused
  - dd86e0d78 GREEN refusal in generate_add_type_patches
  - 3e7b3ac3c refactor preflight uses CRATE_DIRS

- item 4 (AUTOFIX6 list):
  - a84cf4796 RED function_diff::tests::{a_borrowed_return_added_by_name_is_cloned,
    an_option_argument_crosses_as_its_ffi_option, a_slice_argument_crosses_as_its_vec_slice,
    an_accessor_never_hits_an_argument_whose_name_ends_with_another}
  - 442116648 GREEN templates + slice_arg_type; 981555c7f GREEN option_arg_ffi_type
    (+ written_type source_ty; wildcard test now includes set_link); 9defb80c5 GREEN clone
  - 5230fcf9c RED patch::tests::a_patch_that_only_removes_is_not_empty_and_creates_no_class
  - e8004ed34 GREEN has_removals / removes_only / is_empty / is_path_only
  - 68499bb14 RED pending::tests::an_add_after_a_pending_removal_of_the_whole_class_replaces_its_entries
  - d868a9bfa GREEN prepare_add (+ main.rs both add arms)

- item 5: already done at base (0a98cab8f F1, F17); verified (prebuilt `autofix modules`: all
  types in the right module; no widget VecSlice outside widgets/shells):
  - f40e56da5 regression guards (mod.rs macro_path_tests source scan; CellGridRangeVecSlice case)
- 4afb8b3cc fix: private_module_on had fallen under add_type_for_test's #[cfg(test)]

## IN PROGRESS
- final review pass of the diff vs 2e55eef06 for compile errors (done: add.rs, function_diff.rs,
  type_index.rs; NEXT to review: mod.rs rest, diff.rs, patch/mod.rs, pending.rs, main.rs)

## NEXT
- finish the review pass (above), fix + commit anything found
- write the report scripts/TOOLS7_2026_10_03.md (built, commits, api.json list, least-sure spots,
  test commands, what is left) and commit it

## Decisions / open questions
- item 0: root cause = file-name collision in the new-type add path. Fixed by naming.
- item 1: free-fn receiver passed as receiver_arg_name (`raw_image`); *CallbackInfo by value.
- item 3: no `--module` flag: the scan's move check would move a hand-picked module back unless
  the exceptions table names it; the table IS the persistent choice. Path-first placement was
  rejected (1223 api.json classes sit in a module other than their path's - by-concern modules
  like component/font/time); new types follow the move check's order instead.
- item 3: TextRasterStyle moves css -> image at the next scan; css/src/codegen/lower_types.rs
  (generated) must be regenerated; AzPhoto import already switched.
- commit messages: Write tool -> /tmp/tools7_msg.txt; git via `git -C`.
