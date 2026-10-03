# TOOLS7 progress (branch wt/tools7, base 2e55eef06)

## DONE (all items; full commit list in scripts/TOOLS7_2026_10_03.md)
- item 0: f07526e27 / f044740e7 (file names), 9a9a744c5 / ddf300767 (dropped_entries)
- item 1: dce36deda, d36edd384, 74ec2c71f, a88f2ce23, cd2b046b9, eafac9034 (`--fn`, CallbackInfo by value,
  multi-spec add)
- item 2: 25c3f85ba / e5de1ad90 (BareObjectInFnBody), 89c1fc76c / a3d68900c (destroy bodies)
- item 3: ff9d487db / 5c2d874a6 (new_type_module, TextRasterStyle -> image), 3c1d0d238 (AzPhoto import),
  e09122405 / 2d2e48396 (public re-export paths), c11f9e711 / 1b4de30bd (path fix + PrivateExternalPath),
  1a1d8ef44 / dd86e0d78 (add refuses), 3e7b3ac3c (CRATE_DIRS), 4afb8b3cc (cfg fix)
- item 4: a84cf4796, 442116648, 981555c7f, 9defb80c5, 5230fcf9c / e8004ed34, 68499bb14 / d868a9bfa
- item 5: verified done at base; f40e56da5 regression guards
- review pass of the whole diff done; report scripts/TOOLS7_2026_10_03.md committed

## IN PROGRESS
- nothing

## NEXT
- nothing (task finished); the parent: scan + apply (TextRasterStyle css -> image), regenerate css lowering,
  run the test commands in the report

## Decisions
- item 0 root cause = file-name collision in the new-type add path.
- item 1: free-fn receiver = receiver_arg_name (`raw_image`); *CallbackInfo by value.
- item 3: no `--module` (the exceptions table is the persistent choice); path-first rejected (1223 classes
  live by concern); new types follow the move check's order.
- commit messages: Write tool -> /tmp/tools7_msg.txt; git via `git -C`.
