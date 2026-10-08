# REFACTOR13 - split solver3/fc.rs + text3/cache.rs into modules, shrink layout_bfc's frame

Worktree branch: started on `fix/input-bugs-2026-09-19` @ 02cc380e4, brought up to date by
`git merge --no-ff fix/input-bugs-2026-09-19` @ de39fa699 (merge 91be7443b). fc.rs and
text3/cache.rs are byte-identical on both; solver3/cache.rs gained 15 lines
(`apply_content_based_height`).

Method (user): safety net first, then tools move the code verbatim, then only plumbing is
hand-written, and scripts prove nothing else changed.

Builds: `scripts/refactor/locked_cargo.sh cargo ... -j 4` (machine-wide lock, own target dir
`/Users/fschutt/Development/azul-refactor-target`, `CARGO_PROFILE_{DEV,TEST}_DEBUG=0`,
`CARGO_INCREMENTAL=0`, never below 6 GB free).

## Phase A - safety net (BEFORE any code moves)

### Baseline on the merged tip (91be7443b = de39fa699 + this branch's new tests; debug)

| suite | listed | passed | failed | ignored |
|---|---|---|---|---|
| `cargo test -p azul-layout --lib` | 9518 | 9518 | 0 | 0 |
| `cargo test -p azul-layout --test all` | 2297 | 2289 | 1 | 7 |

`--test all` = the tip's own 2290 tests (2283 pass, 7 ignored) + this branch's 7: the 6 golden
tests (pass) + the depth test (RED by design: `thread '<unknown>' has overflowed its stack` in
its child, after "deep chain: styled"). At the end: identical names, 2290 passed, 0 failed.

Full sorted name lists (libtest `--list`, `: test` stripped), too big for this file:
- `scripts/refactor/baseline/lib_tests.txt` (9518 names, sha256 03fbcec66571a95f...)
- `scripts/refactor/baseline/all_tests.txt` (2297 names, sha256 c4e72852053b932d...)

The 7 ignored: a_rebuild_of_an_unchanged_page_costs_little,
flex_intrinsic_text::frame_around_overflow_hidden_strip_shrinks_too,
image_child_paint::dbg_image_child_tree, inline_atomic_after_block::dbg_dump_two_passes,
ribbon_tab_whitespace::probe_tab_wrap_and_caption_centering_across_widths,
rice_styles_the_window::a_widgets_rice_beats_a_widgets_static_inline_property,
svg_paint::a_parents_hover_can_restyle_a_child.

(First base 02cc380e4: lib 9489/9489, all 2266 listed, 2259 passed, 7 ignored.)

### Goldens (`layout/tests/golden/*.txt`, 1.0 MB, written ONCE with AZ_GOLDEN_WRITE=1)

| group | documents |
|---|---|
| mail_corpus (lenient HTML loader) | 20 |
| wpt_css (normalized WPT, XML loader as the reftest runner) | 281 |
| wpt_html_and_local | 93 |
| purpose_built | 20 |
| paged (6 documents x slicer + break tokens) | 12 |
| widgets (status bar, ribbon, list view) | 3 |

Determinism: matched the goldens in two further processes (parallel, `--test-threads=1`) and
inside the full `--test all` run. Recorded as-is: one engine panic (see Bugs).

### Frame sizes before (`scripts/refactor/frame_sizes.py <binary>`; prologue `sub sp` incl. the
stack-probe target, register pushes excluded)

| function | debug (debuginfo=0) | release (`cargo build --release -p azul-layout --lib`) |
|---|---|---|
| solver3::fc::layout_bfc | 25,952 (+32 saved regs) | 3,248 (+160) |
| solver3::cache::calculate_layout_for_subtree_fragment | 7,312 (+32) | 1,344 (+160) |
| solver3::fc::layout_formatting_context | 1,104 (+32) | 2,128 (+160) |
| solver3::cache::calculate_layout_for_subtree | 64 | 64 |

Per nesting level: debug ~34.4 KiB + 128 B of pushes; release 6,784 + 480 B. The lead's debug
26,736 / 7,776 / 1,152 / 144 were measured with debuginfo, which spills arguments to extra
slots; release matches the lead's (3,248 / 2,128 / 1,328~1,344). Release is measured on the
rlib (`/tmp/r13/release_before.rlib`, the merged tip).

Other functions on recursive paths (debug, outside the div-chain target): layout_document 24,176
(once per layout); fc::layout_ifc 12,624; fc::layout_table_fc 9,664;
fc::collect_and_measure_inline_content_impl 7,808; fc::collect_inline_span_recursive 5,744;
fc::measure_atomic_inline 3,600; cache::prepare_layout_context 1,888; fc::layout_flex_grid 1,632;
cache::process_inflow_child 1,392; cache::process_out_of_flow_children 976;
sizing::calculate_intrinsic_sizes 832.

## Status

- [x] tools (bac146663): item_index, split.py, extract.py, verify_moved.py, frame_sizes.py,
      locked_cargo.sh; negative controls (changed constant, swapped lines, todo!()) FAIL verify.
- [x] Phase A: golden tests + RED depth test + goldens + merged-tip baseline
- [x] Phase B (ec1a3d78f; tool fixes c7a6ab771): fc.rs -> fc/ (17 modules + 6 outlined test
      modules). verify OK (0 unclassified); lib 9518 pass, all 2289 pass + depth RED + 7 ignored,
      names identical, goldens match; clippy: no new warning (1569 -> 1564, see commit).
- [x] Phase C: layout_bfc (ebeb82804, redone cba7c950a for clippy-clean signatures) 25,952 ->
      2,144 B debug; reconcile_recursive (2ff6f3151, the next depth limit, found with lldb)
      15,184 -> 6,416 B. Depth test green; deepest chain on 2 MiB: 120 -> 180.
- [ ] Phase D: text3/cache.rs split by script (plan: scripts/refactor/text3_cache_plan.json)

## Blocker log

- 06:05-06:33: free disk 2.3-5.8 GB (< 6 GB floor); stopped and reported. 06:4x: the coordinator
  freed the disk (24 GB) and asked for the merge above; resumed.

## NEXT

Phase D: `python3 scripts/refactor/split.py scripts/refactor/text3_cache_plan.json`, verify
(`--old layout/src/text3/cache.rs --new layout/src/text3/cache --plan ...`), clippy (no new
warnings), privacy errors -> `member_visibility` in the plan, re-run; then both suites, release
frames, final report. (Uncommitted scratch: layout/tests/zz_scratch_depth_probe.rs + its all.rs
line - never stage them.)

## Bugs noticed (not fixed - the refactor never changes behaviour)

- `wpt/normalized/css/css-text-decor/reference/text-decoration-subelements-003-ref.html` panics
  in layout: `get_used_text_color: node 22 has no color in its resolved style although the UA
  pass ran - the themed root default did not reach it` (the golden records the panic).
