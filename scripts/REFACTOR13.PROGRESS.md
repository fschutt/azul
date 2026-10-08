# REFACTOR13 - split solver3/fc.rs + text3/cache.rs into modules, shrink layout_bfc's frame

DONE. Branch: started on `fix/input-bugs-2026-09-19` @ 02cc380e4, brought up to date by
`git merge --no-ff fix/input-bugs-2026-09-19` @ de39fa699 (merge 91be7443b).

Method (user): safety net first, then tools move the code verbatim, then only plumbing is
hand-written, and scripts prove nothing else changed.

Builds: `scripts/refactor/locked_cargo.sh cargo ... -j 4` (machine-wide lock, own target dir
`/Users/fschutt/Development/azul-refactor-target`, no debuginfo unless `LOCKED_CARGO_DEBUG`,
`CARGO_INCREMENTAL=0`, never below 6 GB free).

## Tests: before = after

| suite (debug) | baseline (merged tip + Phase A) | after Phase D |
|---|---|---|
| `cargo test -p azul-layout --lib` | 9518 passed | 9518 passed, names identical |
| `cargo test -p azul-layout --test all` | 2297 listed: 2289 passed, 1 failed (the RED depth test), 7 ignored | 2297 listed: 2290 passed, 0 failed, 7 ignored, names identical |

Name lists: `scripts/refactor/baseline/{lib,all}_tests.txt`. `--test all` = the tip's own 2290
(2283 pass + 7 ignored) + 6 golden tests + the depth test. Goldens (1.0 MB,
`layout/tests/golden/`) matched after every commit of Phases B-D; written once (a471bba42).

## Frames (`scripts/refactor/frame_sizes.py`, prologue `sub sp` + probe; saved registers apart)

| function | debug before | debug after | release before | release after |
|---|---|---|---|---|
| fc::layout_bfc | 25,952 | 2,144 | 3,248 (+160) | 640 (+144) |
| fc::bfc_prepare (new) | - | 496 | - | 336 |
| fc::bfc_place_children (new, between recursions) | - | 23,968 | - | 2,704 (+160) |
| cache::calculate_layout_for_subtree_fragment | 7,312 | 7,312 | 1,344 (+160) | 1,360 (+160) |
| fc::layout_formatting_context | 1,104 | 1,104 | 2,128 (+160) | 2,096 (+144) |
| cache::calculate_layout_for_subtree | 64 | 64 | 64 | 80 |
| cache::reconcile_recursive | 15,184 | 6,416 | 3,616 (+96) | 1,008 (+96) |
| cache::reconcile_node (new) | - | 9,824 | - | 3,056 (+96) |
| cache::classify_reconciled_node (new) | - | 496 | - | 288 |

debug = no debuginfo; with `debug = 2` (the dev default): layout_bfc 2,256, fragment 7,776, lfc
1,152, subtree 144, reconcile_recursive 6,768.
Per level of the block path: debug 34.4 KB -> ~10.7 KB; release 7,264 -> 4,624 B.
Deepest div chain on a 2 MiB thread (scratch probe, child process per depth): 120 -> 180
(debuginfo=0), 160 with debuginfo=2; past that the limit is the layout pass's
calculate_layout_for_subtree_fragment + layout_formatting_context + layout_bfc (lldb).
Release inlining moves with the codegen-unit partition (by module): after Phase B
layout_table_fc / layout_flex_grid were separate frames, after Phase D table is inlined into
layout_formatting_context again. The IFC re-layout of the second pass recurses below
layout_bfc + bfc_place_children: +240 B release / +160 B debug per such level vs the one
layout_bfc frame before (follow-up: split bfc_place_children's per-child body around it).

## Commits

| commit | what |
|---|---|
| bac146663 | tools: item_index, split.py, extract.py, verify_moved.py, frame_sizes.py, locked_cargo.sh |
| 811e2bde6 | Phase A WIP: golden tests, RED depth test, baseline lists, plans |
| 7f715c09e | progress (disk), fc plan member visibility |
| 91be7443b | merge fix/input-bugs-2026-09-19 @ de39fa699 |
| a471bba42 | Phase A: goldens written once, mail through the lenient loader, merged baseline |
| fba9824bd | release frames before; frame_sizes reads .rlib |
| c7a6ab771 | tools: `super::x` -> `super::super::x`, re-exports no wider than their items |
| ec1a3d78f | Phase B: fc.rs -> fc/ (generated) |
| 14ef4bfa5 | progress |
| ebeb82804 | Phase C: layout_bfc extraction (first signatures) |
| cba7c950a | Phase C: the same extraction redone - helpers take borrowed values by value (clippy-clean) |
| 2ff6f3151 | Phase C: reconcile_recursive extraction (the next depth limit) |
| (progress) | |
| 41fe1b0de | tools: member visibility across same-named items; text3/cache plan members |
| 0b018d6c0 | Phase D: text3/cache.rs -> cache/ (generated) |

## Regenerating on a newer base (the order matters)

Every generated commit reproduces byte for byte from the committed plans (checked: split of
c7a6ab771's fc.rs == ec1a3d78f's fc/, split of 41fe1b0de's cache.rs == 0b018d6c0's cache/,
bfc_extract on ec1a3d78f == cba7c950a's bfc.rs, reconcile_extract on cba7c950a ==
2ff6f3151's solver3/cache.rs).

On a base where other branches changed fc.rs / text3/cache.rs / solver3/cache.rs:
1. Take their single-file fc.rs, text3/cache.rs and solver3/cache.rs (a merge into this branch
   conflicts as modify/delete: keep THEIR files, delete layout/src/solver3/fc/ and
   layout/src/text3/cache/). Build item_index (`cd scripts/refactor/item_index &&
   ../locked_cargo.sh cargo build --release --offline`).
2. Goldens first, on the unsplit merged code: `AZ_GOLDEN_WRITE=1 cargo test -p azul-layout
   --test all -- the_layout_of_a_broad_corpus_matches_its_golden_dumps::`, run it twice more
   without the env (determinism), commit. Re-record the baseline lists.
3. `python3 scripts/refactor/split.py scripts/refactor/fc_plan.json` - an item the plan does not
   place is an error: add new items to a module of the plan. Then
   `verify_moved.py --old-rev HEAD --old layout/src/solver3/fc.rs --new layout/src/solver3/fc
   --plan scripts/refactor/fc_plan.json`; `cargo clippy -p azul-layout --all-targets`: a
   privacy error names a field / method for the plan's `member_visibility`; re-run the split
   (from the restored fc.rs) until it compiles; compare clippy's warnings with the unsplit
   run; both suites; commit.
4. `python3 scripts/refactor/extract.py scripts/refactor/bfc_extract.json --manifest
   scripts/refactor/bfc_extract.manifest.json` (its marker lines must still occur once in
   layout_bfc), `verify_moved.py --old-rev HEAD --old/--new layout/src/solver3/fc/bfc.rs
   --blocks scripts/refactor/bfc_extract.manifest.json`; build, frames, suites; commit.
5. The same with `reconcile_extract.json` on layout/src/solver3/cache.rs.
6. `split.py scripts/refactor/text3_cache_plan.json`, verify with `--old
   layout/src/text3/cache.rs --new layout/src/text3/cache --plan ...`, as in 3.

## Bugs noticed (not fixed - the refactor never changes behaviour)

- `wpt/normalized/css/css-text-decor/reference/text-decoration-subelements-003-ref.html` panics
  in layout: `get_used_text_color: node 22 has no color in its resolved style although the UA
  pass ran - the themed root default did not reach it` (the golden records the panic).
- The crate's clippy job (`-D warnings`) is red today: azul-layout alone has 1564 warnings
  (unchanged by this work; 1569 before Phase B, 5 benign ones vanished).
- managers::a11y's deep-chain test could go back to the harness's 2 MiB thread (61 levels).
