# REFACTOR13 - split solver3/fc.rs + text3/cache.rs into modules, shrink layout_bfc's frame

Worktree branch based on `fix/input-bugs-2026-09-19` @ 02cc380e4.
Method (user): safety net first, then tools move the code verbatim, then only plumbing is
hand-written, and scripts prove nothing else changed.

Builds: `scripts/refactor/locked_cargo.sh cargo ... -j 4` (machine-wide lock, own target dir
`/Users/fschutt/Development/azul-refactor-target`, `CARGO_PROFILE_{DEV,TEST}_DEBUG=0`).

## Phase A - safety net (BEFORE any code moves)

### Baseline on 02cc380e4 (debug, no debuginfo)

| suite | listed | passed | failed | ignored |
|---|---|---|---|---|
| `cargo test -p azul-layout --lib` | 9489 | 9489 | 0 | 0 |
| `cargo test -p azul-layout --test all` | 2266 | 2259 | 0 | 7 |

(The lead's 9477 / 2262 were from an older base.) The `--test all` run printed 2258 passed +
1 failed: the failure was `integration_test_registry_is_exhaustive` seeing my then unregistered
new test file in `tests/` (it scans the directory at run time) - not a base failure.

Full sorted name lists (libtest `--list`, `: test` stripped) - they are too big for this file:
- `scripts/refactor/baseline/lib_tests.txt` (9489 names, sha256 19e20bdc23a5...)
- `scripts/refactor/baseline/all_tests.txt` (2266 names, sha256 f68d6786ac8a...)

The 7 ignored (`--test all -- --list --ignored`): a_rebuild_of_an_unchanged_page_costs_little,
flex_intrinsic_text::frame_around_overflow_hidden_strip_shrinks_too,
image_child_paint::dbg_image_child_tree, inline_atomic_after_block::dbg_dump_two_passes,
ribbon_tab_whitespace::probe_tab_wrap_and_caption_centering_across_widths,
rice_styles_the_window::a_widgets_rice_beats_a_widgets_static_inline_property,
svg_paint::a_parents_hover_can_restyle_a_child.

### Frame sizes before (`scripts/refactor/frame_sizes.py <binary>`; prologue `sub sp` incl. the
stack-probe target, register pushes excluded)

| function | debug (debuginfo=0) | release |
|---|---|---|
| solver3::fc::layout_bfc | 25,952 | (pending) |
| solver3::cache::calculate_layout_for_subtree_fragment | 7,312 | (pending) |
| solver3::fc::layout_formatting_context | 1,104 | (pending) |
| solver3::cache::calculate_layout_for_subtree | 64 | (pending) |

Per nesting level (debug) ~34.4 KiB + 128 B of pushes. The lead's 26,736 / 7,776 / 1,152 / 144
were measured with debuginfo, which spills arguments to extra slots; with `debug = 2` (the
workspace dev default) the frames are those, so the depth test keeps headroom for that.

## Status

- [x] tools committed (bac146663): item_index, split.py, extract.py, verify_moved.py, frame_sizes.py,
      locked_cargo.sh. Scratch runs: fc split (170 items -> 17 modules + 6 outlined test modules)
      and text3/cache split (358 items -> 20 modules + 5 outlined) both verify OK; the bfc
      extraction spec verifies OK on the scratch split; negative controls (changed constant,
      swapped lines, todo!()) all FAIL verify as they must.
- [ ] Phase A: golden tests + RED depth test written (WIP commit, NOT yet compiled clean: the
      first compile found 2 errors, both fixed, rebuild blocked on disk); goldens not generated
- [ ] Phase B: fc.rs split by script (plan: scripts/refactor/fc_plan.json)
- [ ] Phase C: layout_bfc extraction (spec: scripts/refactor/bfc_extract.json)
- [ ] Phase D: text3/cache.rs split by script (plan: scripts/refactor/text3_cache_plan.json)

## Blocker log

- 06:0x-06:20: free disk 4.9-5.8 GB (< 6 GB floor) with no build of mine running (the main
  checkout's target is 28 GB); locked_cargo.sh waits for 6 GB with the lock released.
- 06:21: another session's `cargo test -j 4` took the lock; free disk fell to 2.3 GB. I deleted
  my own debug artifacts (1 GB) to give the machine headroom - the next build of mine starts
  cold (deps included). Baseline numbers above are recorded, so nothing is lost.
- 06:33: no build running anywhere, lock free, free disk steady at 4.7 GB (< 6 GB). STOPPED per
  the rule and reported. A cold debug build of the two test binaries needs ~1 GB (no
  debuginfo, no incremental). Resume = NEXT below, from step 1.

Frames of other functions on recursive paths at baseline (debug, debuginfo=0; outside the
div-chain target, for the report): layout_document 24,176 (once per layout); fc::layout_ifc
12,624; fc::layout_table_fc 9,664; fc::collect_and_measure_inline_content_impl 7,808;
fc::collect_inline_span_recursive 5,744; fc::measure_atomic_inline 3,600;
cache::prepare_layout_context 1,888; fc::layout_flex_grid 1,632; cache::process_inflow_child 1,392;
cache::process_out_of_flow_children 976; sizing::calculate_intrinsic_sizes 832.

## NEXT

1. `scripts/refactor/locked_cargo.sh cargo test -p azul-layout --test all -j 4 --no-run` (fix
   compile errors in the two new test files only).
2. `AZ_GOLDEN_WRITE=1 cargo test -p azul-layout --test all -j 4 -- the_layout_of_a_broad_corpus`
   then the same WITHOUT the env twice (determinism: identical, all green), check the depth test is
   RED (stack overflow in the child), commit Phase A (tests + goldens + this file).
3. Phase B: `python3 scripts/refactor/split.py scripts/refactor/fc_plan.json`, then
   `verify_moved.py --old-rev HEAD --old layout/src/solver3/fc.rs --new layout/src/solver3/fc
   --plan scripts/refactor/fc_plan.json`, `cargo check -p azul-layout --tests`, plumbing into the
   plan (member_visibility) and re-run.

## Bugs noticed (not fixed - the refactor never changes behaviour)

(none yet)
