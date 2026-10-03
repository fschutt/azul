Task AUTOFIX6 (azul Rust GUI toolkit, PR #476, wave 6). Read first, in this order, and follow them exactly (never compile):
1. scripts/waves/house_rules.md (the house rules - wave-6 version: building blocks, user rulings, API gotchas)
2. scripts/waves/wave6/PLAN.md (the eleven tasks, who owns which files, the contracts between tasks)
Your branch: `wt/autofix6` from the base commit named in your launch prompt (`git -C <your worktree> checkout -b wt/autofix6 <base>`).
TASK name: `AUTOFIX6`. Report `scripts/AUTOFIX6_2026_10_03.md`, progress `scripts/AUTOFIX6.PROGRESS.md` (commit it after every commit).

GOAL: the six gaps of `azul-doc autofix` found at the wave-5 integration (scripts/waves/wave5/STATUS.md), each a RED
unit test in doc/src/autofix (or doc/src/patch) first, then the fix (the binary's tests: `cargo test -p azul-doc --bin
azul-doc -- autofix::` - the parent runs them):
1. A raw `str` / `&str` / `Option<&str>` RETURN type is not flagged as a critical FFI error (codegen then emits the
   non-existent types `Azstr` / `AzOptionstr` and the dylib does not build).
2. api.json functions whose Rust method is GONE are not detected (BLOCKS removed 84 preset-shell setters; the scan
   kept them until they were removed by hand): report and generate removals for them.
3. A std `String` argument gets no conversion in the generated wrapper (the C side passes AzString): either convert
   (`.into()` in the fn_body) or flag it.
4. `autofix add X.m` while a `remove X.m` patch is pending is skipped (add sees the old entry) - the remove-then-re-add
   of an entry with a changed signature must work in one round.
5. `autofix add Type.*` exports EVERY public method, Rust-only helpers included (RichTextDoc got 29 extra entries,
   RichRun 7): restrict the wildcard (e.g. skip methods taking/returning non-FFI types, `&mut self` helpers returning
   references, methods marked `#[doc(hidden)]`, or a `// api: skip` marker - pick one rule, document it).
6. A struct with repr none in source but `repr: C` in api.json loops on a `set_repr` modify patch every round instead
   of being reported (and removed when unreachable from the API) - `autofix difficult remove` was needed.
Also: a removal of a class whose source file is gone used to target a guessed module (fixed in 178af99a9) - add the
regression test if missing; `autofix remove` takes `module.Type.method` but not `module.Type` (a whole class) - make it.
Files: doc/src/autofix, doc/src/patch only.

Report `scripts/AUTOFIX6_2026_10_03.md` per the house rules (what was built, commits, the api.json list - exact methods,
never `Type.*`; least-sure-to-compile spots; the parent's test commands; what you saw broken and did not fix; what is
left). You are running unattended: decide, note decisions in your progress file, continue; do not stop to ask. Do not
spawn subagents.
