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

## IN PROGRESS
- item 2: bare `object` argument in a fn_body with a self arg = critical scan error

## NEXT
- item 2 RED (mod.rs check_function_signatures neighbourhood), GREEN
- item 3 (module for new types / --module / private module path)
- item 4 (AUTOFIX6 list), item 5 (css/src/macros.rs + module_map VecSlice)
- report scripts/TOOLS7_2026_10_03.md

## Decisions / open questions
- item 0: root cause = file-name collision in the new-type add path (each add overwrote
  add_<type>_<i> / add_<type>_functions). Fixed by naming; the apply-side merge was already right.
- item 1: the receiver of a free fn (first arg of the class type) is passed as the codegen's
  receiver name (receiver_arg_name: `raw_image`), which is the generated C fn's parameter
  (DLL: no rewrite needed; Python rewrites it to __cloned).
- item 1: `&`/`&mut` *CallbackInfo args cross by value (+ `let mut x = x;` rebinding for &mut):
  is_by_value_handle = name ends with "CallbackInfo". Also fixes the drift report on
  ProgressBar.update_progress seen with the prebuilt `autofix list ProgressBar`.
- commit messages: written with the Write tool to /tmp/tools7_msg.txt (heredocs with quotes
  trip the worktree guard); git via `git -C`.
