# TOOLS7 progress (branch wt/tools7, base 2e55eef06)

## DONE
- f6b29c26e progress file
- item 0 (one method per new type):
  - f07526e27 RED test autofix::add::tests::two_add_patches_for_one_new_type_keep_both_methods
    (new module doc/src/autofix/add.rs; main.rs new-type path calls it; AddTypeResult.functions_patch)
  - f044740e7 GREEN: file names say what they hold (add_<t>.type.<x>, add_<t>.impls, add_<t>_<spec>)
  - 9a9a744c5 RED patch::tests::the_apply_reports_a_function_a_later_patch_dropped
  - ddf300767 GREEN patch::dropped_entries (patch/mod.rs, outside the owned list - minimal)

## IN PROGRESS
- item 1: `autofix add Class.name --fn <path::to::free_fn>`

## NEXT
- items 1..5 in brief order

## Decisions / open questions
- item 0: root cause = file-name collision in the new-type add path (each add overwrote
  add_<type>_<i> / add_<type>_functions). Fixed by naming; the apply-side merge was already right.
  Multi-method `autofix add T.a T.b` folded into item 1's restructure of the add command (if done).
