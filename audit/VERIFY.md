# Verification pass of `audit/integration` (verifier, 2026-11-21)

The one pass of the audit that compiles and runs. Everything on this branch below `aeb5d0dfe` was written blind
(about 520 commits, 193 files, +72,000 lines). This file is the log: each command, its result, every commit the
verifier made, every commit it reverted and why. The state comes first (kept current), then the chronological log.

Environment of every build: `~/Development/mini-cc/tools/cg cargo <...> -j 2`, one at a time,
`CARGO_TARGET_DIR=<scratchpad>/azul-target`, `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_{DEV,TEST,RELEASE}_DEBUG=0`,
rustc 1.91.0 (the pinned toolchain), macOS aarch64. Logs: `<scratchpad>/verify/<nn>-<name>.log` (not committed);
helpers there: `env.sh` (`run <log> <cargo args>`, `summ <log>`), `errs.py` (errors grouped), `gdiff.py` /
`gsplice.py` (golden dumps per document).

## State (kept current)

| crate | compiles | tests compile | suites (last full run) |
|---|---|---|---|
| azul-css | yes | yes | `cargo test -p azul-css --no-fail-fast`: lib 2942 passed, 0 failed; 14 integration targets passed |
| azul-core | yes | yes | `cargo test -p azul-core`: lib 3119 passed, 0 failed; 30 integration targets + doc tests passed |
| azul-layout | yes (default features and `--features e2e-server`) | yes (`--lib --tests`) | run 3 (head `57a46db75`): lib 10867 passed / 2 failed / 1 ignored; `tests/all` 3059 passed / 1 failed / 5 ignored (names in the log) |
| azul-dll | yes: default features, the editor's set (`--no-default-features --features link-static,serde-json,png`) and that + `e2e-scripting`: 0 errors each, no fix needed | yes (`--lib --tests`) | `cargo test -p azul-dll --lib` (head `57a46db75`): 3834 passed / 16 failed / 9 ignored (table "dll lib failures"); `headless_lifecycle`, Linux / Windows checks, fork tests: NOT RUN yet |

Compile errors fixed so far: 17 (16 in the layout lib, 1 in a layout test), all classes listed in the log.

The gate of the base (`e02646d9a`; from the previous session's `gate-b3.sh` and its logs, the numbers of the commit
message were attached to the wrong suites):
- `cargo test -p azul-layout --features e2e-server --lib --test all`: lib 10404 passed, 1 ignored; `all` 3060 passed, 5 ignored
- `cargo test -p azul-dll --lib` (default features, generated sources present): 3731 passed, 9 ignored
- `cargo test -p azul-dll --test headless_lifecycle`: 8 passed
- `cargo check -p azul-dll --target x86_64-unknown-linux-gnu` and `--target x86_64-pc-windows-gnu`

Merged into `audit/integration` by the verifier (all without conflict unless said):
- round 1: `audit/A6` @30f902802, `audit/A1` @f2ecfda91, `audit/A4` @8a7af63ed, `audit/A10` @7d549eebe,
  `audit/A3-window` @914e54179, `audit/A3-solver` @ceb7587af.
- round 2: `audit/diff-first` at `cfc4d6d72` (exactly), `audit/A3-window` at `257052358` (exactly), `audit/A1`
  @dcccedd33 (conflict in `cpurender/retained.rs` `put_back`: both sides kept: A1-26's bound is the condition,
  A10-06's stamp of a repaired surface is in the body), `audit/A10` @21a441eee, `audit/A3-solver` @9bdd811f2,
  `audit/A4` @921340f35, `audit/A5` @8a9e74233.
- round 3: `audit/A3-solver` at `7f6081e84`, `audit/A5` at `844d40676`.
- round 4: `audit/A1` at `6ec61ce06` then `b23bfc358` (A1-62 .. A1-65), `audit/A3-solver` at `fc4ffd21d`,
  `audit/A10` at `e45d1da96`, `audit/A4` at `65e8bfa2e`, `audit/A3-window` at `022638f7e` then `d68713628`.
- NOT merged (the coordinator's word is needed): `audit/diff-first` beyond `cfc4d6d72`, `audit/A3-window` beyond
  `257052358`, `audit/dom-builder`, `audit/A8`'s successor work.

Generated code:
- `css/src/codegen/lower_types.rs` (tracked): current (`python3 css/tools/gen_codegen_lowering.py`: no change after
  any merge round since `5db5f406d`).
- `<worktree>/target/codegen` (untracked; what `dll/build.rs` checks and `dll/src/lib.rs` includes), made from
  the `api.json` of head `f55c9cd9f` (unchanged by every merge since): CURRENT are the Rust outputs
  `dll_api_internal.rs`, `dll_api_external.rs`, `reexports.rs`, `python_api.rs`, `php_api.rs`, `memtest.rs`,
  `azul.rs`, and `api.json.br` (made here with the `brotli` CLI, quality 11, from the minified `api.json`: the
  generator writes it last) and `material_icons.ttf.br` (copied from the editor's codegen directory: the same font).
  How: `cargo build -p azul-doc` (dev profile; 0 errors), `<target>/debug/azul-doc codegen all` from the worktree
  root, STOPPED after step 13 of 35 (memtest): the unoptimised generator needs minutes per language binding and
  its 0.9 GB of output would have taken the disk under the floor.
- STALE / not made in this pass: every other language binding of `codegen all` (C and C++ headers `azul.h`,
  `azul*.hpp`, `azul.cppm`; C#, Lua, Pascal, PHP source, Nim, Odin, D, Crystal, Julia, Lisp, Ada, ... : steps
  14-35). The pin pass / a release must run the whole generator:
  `cargo build -r -p azul-doc -j 2 && <target>/release/azul-doc codegen all` from the repository root (as
  `mapedit3/scripts/azul-codegen.sh` does): about 0.9 GB of output in `target/codegen` plus the release build of
  `azul-doc` (a few GB of target). The api.json changes it has to pick up: `TextHaloStats` +
  `CallbackInfo.get_text_halo_stats` (A5), `FrameTimings` + 24 `gpu_*` fields (A4, the last is `gpu_tiles_drawn`),
  `Dom::with_virtual_view_version` / `set_virtual_view_version` (A3-window), `SharedBytes` +
  `CallbackInfo.register_font_shared` (A8), `StyledDom::focusable_nodes` / `nodes_with_markers` (A6).

## Reverted commits

- `0a88603d3` A8-28 (a font stack's chain key carries the px size only when its chain is drawn differently by it),
  reverted by `1e0a87890`. The size-free keys are remembered for the PROCESS, on the premise that a stack resolves
  to the same faces in every font cache. A cache without the system's variable face (an empty `FcFontCache`, one
  the registry has not filled yet) marks `system-ui` size-free; every other cache then draws all sizes from the
  chain of the first size resolved (one optical size of San Francisco for all). Pre-existing
  `system_ui_text_is_as_wide_as_in_chrome` and `text_that_only_changes_size_is_drawn_at_the_new_size` failed in
  the full run (13px: azul 102.875, Chrome 116.266) and passed alone. Not fixable locally
  (`FontChainKey::from_selectors` has no cache to ask). A8-28 / A9-10 are open again; routed to the text owner.

## Code changed by the verifier (beyond compile fixes)

- `6cdc7a4b4` `cpurender/pixmap.rs` `write_region`: a side past `i32::MAX` writes nothing, as the per-pixel loops
  A1-57 (`7c0cb7f4e`) replaced did (pre-existing `write_region_u32_max_dimensions_are_a_noop`).

## Tests changed by the verifier (the code under them is unchanged; each keeps what it guards)

Pre-existing tests whose expectation an area changed on purpose (the area's notes name the change):
- `b87fca8af` css `box_shadow` `scale_for_dpi_quantizes_below_the_fixed_point_precision`; `7c79f8fe9`
  `widgets/card.rs`, `widgets/frame.rs` `flex_grow_below_..._truncates_to_zero` (now `..._below_half_..._rounds_to_zero`):
  A3-core-14 `fb8751e41`, `FloatValue::new` rounds; these pinned the truncation through computed values.
- `4f2c8fb32` `raster.rs` `image_mask_clip_with_a_degenerate_rect_is_skipped`: A1-05 `569d3e0a3`, every mask push
  leaves an entry (an empty one here).
- `0f066dc70` `tests/struct_sizes.rs`: `LayoutNodeCold` 344 -> 360 (`NodeDataFingerprint` gained
  `inline_inset_hash`, `inline_gpu_hash`: A3-window-08 / A3-solver-41).
- `0f066dc70` `tests/selection_skips_unselectable_text.rs` (2), `tests/zero_width_selection.rs` (1): the answer of
  `process_mouse_drag_for_selection` says whether the selection changed (A6-14 / `2bfe90fac`); a drag that ends
  where it began answers `None`. The state after the drag is still asserted.
- `5ec2c0575` golden dumps, 17 documents, each traced to a named change: A3-solver-32 (exactly the four documents
  and lines its commit predicts), A3-solver-48 (list items' `ic=`, and `list-style-position-001-notref.html`
  reflowed by it), A3-core-24 (`border-spacing` in em: `separated-border-model-007/-008`), A3-core-14
  (`mail_corpus/exploration/03_outlook_reply.html`: 1/64 px).

- (after merge round 4) `cpurender/compositor.rs` autotests
  `a_change_inside_a_view_inside_a_scaled_frame_is_damaged_through_the_live_matrix` (40x20 @ (120,100) ->
  50x30 @ (115,95)) and `the_damage_inside_a_view_goes_through_the_frames_around_the_view` (80x60 @ (100,80) ->
  90x70 @ (95,75)): A1-64 `6ec61ce06` grows a rect mapped through a composited frame by 2 px x its scale + 1 px
  where it lands: 2 x 2 + 1 = 5 px a side, exactly; the arithmetic is in the tests' comments.

- (second verifier, `270e178c9`) `audit_tests/a3_window.rs`
  `a_pointer_swept_over_labels_it_never_touched_writes_values_and_changes_no_list`: its last comparison is between
  the lists of TWO windows, and `TransformKey::unique()` is a process-wide counter: 13 `PushReferenceFrame` lines
  of 188 differed, only in the key id (4702.. against 5008..); with the id blanked 0 of 188 lines differ. That one
  comparison now blanks `TransformKey { id: .. }` on both sides (`without_transform_key_ids`); nothing else of
  the test changed (the coordinator's word).

Audit tests with a wrong premise (blind):
- `34fc04d1f` `unified_layout.rs` test helper: E0597 (compile).
- `4f2c8fb32` `raster.rs` `a_repaint_in_bands_is_the_repaint_of_one_pass`: asked `items_walked == 3 x` the one pass;
  a band steps over frames that cannot paint in its rows (A1-07 (h)): 364 against 3 x 173. Now `> 1 x` and `<= 3 x`.
- `4f2c8fb32` `fontreg.rs`: two tests registered different faces under one normalized family name.
- `4f2c8fb32` `shaping_cache.rs` `a_pass_over_more_paragraphs_...`: the shaped store's key holds the process-wide
  registry generation; other tests register faces meanwhile. Counted only when the registry stood still.
- `4f2c8fb32` `e2e/runner.rs` `a11_a_resize_in_the_runner_copies_no_layout_tree`: resized past the resize policy
  (`apply_user_change` + `run_frame`); goes through `service` now.
- `4f2c8fb32` `e2e/cpu_view_values_tests.rs` `a6_25_...`: counted the values a second backend collected.
- `b6434a0cf` `audit_tests/a3_solver.rs` `labels_moved` (4 tests): the labels' insets were `Dom::with_css` strings
  (a scoped sheet: by design not the inset path); now the node's typed style (`with_css_props`).
- `b6434a0cf` `audit_tests/a3_window.rs`: a view sized by insets alone (a replaced box: 300 x 150); "cab cab"
  after "abc" (the space is a new character).
- `b6434a0cf` `audit_tests/a2_cpu.rs` `Stage::write`: asked `Paint` of a write of the value the node has
  (`Unchanged`).

## Open (failing in the tree on purpose; with the owner)

Each fails alone and deterministically; none of `AZ_CPU_THREADS=1`, `AZ_CPU_RETAINED_MB=0`,
`AZ_CPU_TURNED_TEXT=group`, `AZ_CPU_FULL=layers`, `AZ_NO_ABS_INTERIOR_REUSE=1`, `AZ_DIFF_FIRST=0`,
`AZ_NO_DL_PATCH=1` changes any of them (run 1).

| # | test | what it shows | owner |
|---|---|---|---|
| 1 | `audit_tests::a2_cpu::a2_cpu_the_damaged_render_is_the_full_render_with_an_overlay_and_without_a_ground` | dpi 96, seed 11, ground, badge, step 2 ("pan to (37.25, -60.25) + translate(12, 0) scale(3) + veil at 50 %", "veil at 20 %", "pan to (-120.5, -155)"): 59 px differ from a fresh render, first at (48, 54); damage = [304x96 @ (48,144), 159x200 @ (193,40), 32x20 @ (162,125)] | A1 (+ A2-cpu) |
| 2 | `audit_tests::a2_cpu::a2_cpu_the_damaged_render_is_the_full_render_over_random_pans_zooms_fades_and_edits` | dpi 96, seed 1, ground, no badge, step 19 "turned box: translate(-40, -40) rotate(-45deg)": 1 px differs, at (64, 138); damage = [79.28203x57.32051 @ (65,121), 57.71068x55.142136 @ (48,40)] | A1 (+ A2-cpu) |
| 3 | `cpurender::raster::audit_a1_tests::a_turned_label_of_a_view_is_drawn_without_a_surface` | last assertion "damaged == full": 5 px differ after a damaged repaint of the strip (100,60) 130x90 | A1 |
| 4 | `audit_tests::a3_solver::the_layout_of_a_label_takes_what_its_measure_shaped` | kept >= 200, taken = 0: `MeasuredParagraphs::take` never matches | A3-solver |
| 5 | `audit_tests::a3_solver::sizing::a_later_turn_of_the_layout_loop_measures_no_label_whose_inputs_stayed` | its harness line: 1 turn where it expects a second | A3-solver |
| 6 | `the_layout_of_a_broad_corpus_matches_its_golden_dumps::the_web_platform_css_fixtures_lay_out_as_their_golden_dump` | four documents not updated (coordinator's decision): (a) "failing, pending fix: font-size in absolute units": `CSS2/linebox/vertical-align-sub-001(.xht, -ref.xht)`, `vertical-align-super-001.xht`: `div { font-size: 1in }`, line box 96 px -> 1 px (the glyphs were 1 px before and after: `fs=1`); after the fix the dumps should read `fs=96` and a 96 px line box: update the goldens THEN. (b) "failing, pending fix: float fit tolerance": `css-backgrounds/background-gradient-subpixel-fills-area.html`: the tenth float of 39.6875 px wraps in a 396.875 px box since `FloatValue` rounds | (a) text owner (A8), (b) A3-solver |

| 7 | `audit_tests::a3_window::a_label_moved_where_it_lies_stands_as_a_fresh_layout_puts_it` (round 2) | PANIC in the solver's check, `positioning.rs:1354`: "the absolutely positioned box 5 was placed from its last solve at (175, 31) with the size 60x12, and solving it now gives (181.5, 36.25) ...: an input of the solve is missing in `cache::AbsPlacement`". Passes with `AZ_NO_ABS_INTERIOR_REUSE=1`: the move in place (`50dda7e4e`) leaves the moved box's placement memo | A3-window x A3-solver-14b |
| 8 | `audit_tests::a3_window::a_decoration_written_from_a_callback_is_painted_into_the_list` (round 2) | `a3_window.rs:2620` "a decoration is a repaint": `RebuildDisplayList`, expected `Paint` | A3-window (`3b50c52b0`) |
| 9 | `audit_tests::a3_window::a_patched_frame_of_a_colour_fade_copies_no_list` (round 2) | `a3_window.rs:2582` "harness: the fade's frames are patches (0)" | A3-window |
| 10 | `cpurender::raster::audit_a1_tests::a_walk_through_the_grids_paints_the_items_of_the_walk_that_tests_every_item` (round 2) | seed 11, view, dpi 1, damage 44.5x106.5 @ (495.5, 254.5): the grid walk paints 25 items, the every-item walk 26 (item 754 of the view's list is missing) | A1 (A1-07b `525d647fc`) |

Rows 4, 5 and 6 are fixed since merge round 3, rows 3 and 10 since round 4 (see the log). Rows 1, 2 (A1-62, A1-64
merged; not confirmed yet) and 7, 8, 9 (A3-window `4856559a5`, `91ca03104`, `022638f7e` merged; not confirmed yet)
are decided by run 3.

### dll lib failures (first run of `cargo test -p azul-dll --lib`, head `57a46db75`): 16, none triaged to the end

Messages verbatim: `<scratchpad>/verify/31-failures.txt`. Switch runs were made with the built test binary.

| test (under `desktop::`) | message (short) | what the switches say / likely owner |
|---|---|---|
| `shell2::headless::tests::dragging_the_slider_leaves_no_thumb_behind` (pre-existing) | the thumb did not follow the drag: at (126, 44), expected x~192 | PASSES with `AZ_DIFF_FIRST=0`; fails with `AZ_DIFF_FIRST=text` and `text,nodes`: a defect of diff-first step `text` (`dd25f38e7`) or of what it meets: diff-first owner. A REAL behaviour defect on the default path |
| `shell2::headless::tests::a_rebuild_down_the_diff_first_path_leaves_what_the_full_path_leaves`, `a_rebuild_that_changed_nodes_leaves_...`, `a_rebuild_that_styled_its_changed_nodes_leaves_...`, and the two `..._under_the_pointer_...` (5, diff-first's equality walks) | one line differs by the last bit of an f32: "hit area 14x14 @ (2, 59.8)" vs "(2, 59.800003)" | the first PASSES with `AZ_NO_DL_PATCH=1`: the move in place / patch path adds a delta where the full path solves anew: diff-first x A3-window |
| `shell2::headless::tests::a_changed_text_takes_the_full_path_where_the_reconciliation_has_work` | "a handler: the full path" | diff-first owner |
| `shell2::headless::tests::idle_cpu::a_visible_spinners_frame_damages_only_its_own_rect` (pre-existing) | a spinner frame damaged 66.07x66.07 @ (-9.03, -1.03), outside the spinner's own 32x32 @ (8, 16) | no switch changes it; 66.07 = 46.7 x sqrt 2: the bounding box of a turned box turned again: A1 (A1-60 / A1-64 damage through a turned frame) |
| `shell2::headless::tests::every_holder_of_a_nodes_data_is_repointed::every_holder_of_a_nodes_data_is_reached_by_the_repointing_walk` (pre-existing source guard) | two test fixtures hold a `LayoutWindow` outside the walk: `dll/src/desktop/audit_tests/a5.rs Recorder: window`, `layout/src/audit_tests/a10/layer.rs Window: lw` | mechanical: list both in the guard's `NOT_WALKED` with the reason "test fixture" (A5, A10) |
| `shell2::macos::headless_gpu::retained_list_tests::a_pan_inside_the_margin_is_a_scroll_offset_and_nothing_the_host_pays_for` (pre-existing) | "ten pan frames translated no list": 10, expected 0 | no switch changes it (`AZ_WR_VIEW_SLICE=0`, `AZ_WR_ZOOM_TILES=0`, `AZ_WALK_GPU_VALUES=1` tried): a GPU-path pan sends a list per frame: A4. A REAL cost regression if the test's premise still holds |
| `...retained_list_tests::leaving_the_margin_translates_the_map_alone_and_holds_the_host` (pre-existing) | "the map's new list, and only it": 2, expected 1 | no switch changes it: A4 |
| `...retained_list_tests::a_view_the_frame_does_not_send_again_is_still_drawn` (pre-existing) | "a colour is a rebuild": `Paint`, expected `RebuildDisplayList` | A3-window `3b50c52b0` / `91ca03104` make a decoration write `Paint` on purpose: the expectation is the owner's to restate (does the rest of the test still hold?) |
| `...retained_list_tests::the_hover_of_a_label_in_the_map_sends_the_maps_list_alone_and_the_host_stays_held` (pre-existing) | "one list rebuilt: the map's": 0, expected 1 | A3-window `727d2ba8b` (a hover that comes again is a value) changed guards on purpose; this one was not restated: owner |
| `audit_tests::a2_gpu::a_panned_views_content_reaches_webrender_where_the_cpu_paints_it` | the stripe's spatial node is `SpatialId(2, ..)`, expected `SpatialId(1, ..)` | PASSES with `AZ_WR_VIEW_SLICE=0`: A4-08 `9e3279ed8` puts a view's content under a scroll frame of its own on purpose: A2-gpu / A4 restate |
| `audit_tests::a4::a4_the_frames_after_a_settle_send_pictures_and_no_list_at_1x` | "2 follow-up frames after the settle translated 63 items": (1, 63), expected (0, 0) | A4's notes call this count fragile (glyph raster); the 2x twin passes: A4 |
| `audit_tests::a4::a_settle_sent_without_the_wait::a4_31_a_settle_sent_without_the_wait_hands_over_the_same_things_in_the_same_order` | "a display list waits only when the pass must be in step": (true, true), expected (false, false) | A4 (A4-31) |

Other things found, routed by the coordinator:
- A node placed by a per-node `Dom::with_css` string (the editor's tiles and symbols, `map.rs` 673-808) does not
  get A3-solver-41's inset path; only `with_css_props` nodes do.

## `[reveals]` tests

Not decided yet (each needs its fix reverted in a scratch build to see the test fail without it): A3-solver-32
(`d9e4f6da1` / fix `1e8fd3a32`), -44 (`9943af97e` / `88ad0aa7b` + `e193ca69f`), -45 (`be2c9aff1` / `aeb67d400`), -46
(`318ecf506` / `a3ddc3b26`), -52 (`5a31c9af8` / `d14e782a9`), -53 (`6f54a959e` / `9019db74d`), -54 (`5c0073540` /
`6f402e476`). All seven pass WITH their fixes (run 1). A3-solver-32's fix moved exactly the golden lines its
commit predicted, so that bug was real.

Checkpoint results so far (head `e19bed682`): rows 1 and 2 PASS (`audit_tests::a2_cpu` + `audit_tests::a8`: 20
passed, 0 failed, with `91ca03104`, A1-62 / A1-64 and the harness fix `8193ae482`); A8 `1b3cd3449` merged
(`b419ea882`), 0 compile errors; goldens: exactly the three `vertical-align` documents changed as described and
were taken (`e19bed682`), golden suite 6 of 6. OPEN on this tree: the pre-existing
`a_measured_label_flows_its_text_only_at_a_new_width::a_hovered_label_flows_its_text_once_however_often_it_is_measured`
(3 flows for 1; passes with `AZ_NO_TAFFY_MEASURE_JOURNAL=1`: A3-solver 17c `083fe5ada`) and the 16 dll lib failures.
The gate runs as one background chain writing `<scratchpad>/verify/gate.txt` (one block per step: totals and
failing names; logs `40-gate-layout` .. `46-gate-check-windows`): if this file's log below does not hold its
results, read that file, or run the steps of point 3 again.

### THE GATE on the checkpoint tree (head `e19bed682` + log commits), run as one chain

| suite | command | passed | failed | ignored | failing |
|---|---|---|---|---|---|
| layout lib | `cargo test -p azul-layout --features e2e-server --lib --test all --no-fail-fast` | 10870 | 0 | 1 | none (base: 10404 / 0 / 1) |
| layout `tests/all` | (same command) | 3059 | 1 | 5 | `a_measured_label_flows_its_text_only_at_a_new_width::a_hovered_label_flows_its_text_once_however_often_it_is_measured` (pre-existing; passes with `AZ_NO_TAFFY_MEASURE_JOURNAL=1`: A3-solver 17c `083fe5ada`) (base: 3060 / 0 / 5) |
| css | `cargo test -p azul-css --no-fail-fast` | lib 2942 + every integration target and the doc tests | 0 | | none |
| core | `cargo test -p azul-core --no-fail-fast` | lib 3128 + every integration target and the doc tests | 7 (lib) | | the seven `audit_diff_first_tests` below |
| dll lib | `cargo test -p azul-dll --lib --no-fail-fast` | 3834 | 16 | 9 | the same 16 as the first run (table "dll lib failures") (base: 3731 / 0 / 9) |
| dll `headless_lifecycle` | `cargo test -p azul-dll --test headless_lifecycle` | 8 | 0 | 0 | none (base: 8) |
| Linux | `cargo check -p azul-dll --target x86_64-unknown-linux-gnu` | compiles, 0 errors (1 min 19 s; 392 MB of metadata, deleted after) | | | |
| Windows | `cargo check -p azul-dll --target x86_64-pc-windows-gnu` | compiles, 0 errors (1 min 16 s; 455 MB, deleted after) | | | |
| fork tests | `render_api::audit_a4_tests`, `spatial_tree` | NOT RUN: disk | | | |

core, 7 failing (all new tests of diff-first, `cfc4d6d72`; messages verbatim in `<scratchpad>/verify/42-failures.txt`):
- `diff::diff_test::audit_diff_first_tests::a_rebuild_that_changed_the_tree_or_a_sheet_is_not_named`
  (`diff_test.rs:5375` "premise: the sheet is listed": 2, expected 1),
  `::a_rebuild_that_kept_the_tree_names_the_nodes_it_changed` (`:5333` "the second span's style": `None`, expected
  `Some([4])`), `::the_changed_nodes_are_written_onto_the_kept_arena` (`:5390` "three nodes changed"),
  `::the_subtree_sizes_are_the_tree` (`:5322` "an inline style is no attached sheet"). These read like the premise
  the verifier met in A3-solver's label tests: `Dom::with_css("..")` is a scoped SHEET on the subtree, not the
  node's own style (`with_css_props` is).
- `styled_dom::styled_dom_test::audit_diff_first_tests::the_cascade_over_changed_nodes_answers_what_a_new_cascade_answers`,
  `..._beside_sibling_rules`, `..._under_a_hover`: `styled_dom_test.rs:3369` "seed 1, step 0 (kind 6, picks [38,
  37, 33, 42, 47, 6], ...): 9 differences, the first: Some(\"node 9: matched rules\")": the restricted cascade
  (`009e311d8`, `recascade_nodes`) against a new cascade. Owner: diff-first.

## State for a successor (CURRENT; kept by the second verifier, who took over at `a23a72fde`)

<!-- STATE-BEGIN -->
(see the end of the log: "second verifier")
<!-- STATE-END -->

## State for a successor (the FIRST verifier's hand-over at `a23a72fde`; superseded by the section above)

Tree: `audit/integration`, clean; head = `b419ea882` (merge of `audit/A8` at `1b3cd3449`) + log commits. Everything
the coordinator named for the checkpoint is merged; the second round's queue is listed in the log and must NOT be
merged before the checkpoint. Target directory `<scratchpad>/azul-target` (about 2-4 GB; test binaries and
workspace rlibs are deleted after each suite: `find <target>/debug/deps -maxdepth 1 -type f -perm +111 ! -name
'*.dylib' -delete; rm <target>/debug/deps/libazul*.rlib`). `source <scratchpad>/verify/env.sh` gives `run` (refuses
under 9 GB free) and `summ`. `target/codegen` in the worktree is current for Rust (see "Generated code"); the
`api.json` did not change since.

What is left for the checkpoint, in order:
1. (DONE, see "Checkpoint results so far" above.) Result of log `32-build-layout-tests` + the targeted runs after it (rows 1 and 2 with the corrected harness;
   `audit_tests::a8`; the golden suite: with A8's fix EXACTLY three documents may differ:
   `CSS2/linebox/vertical-align-sub-001.xht`, `-sub-001-ref.xht`, `vertical-align-super-001.xht`: fs=96 on their
   text items, a 96 px line box, baseline 77, the text's width at 96 px: check each, then take them with
   `python3 -I <scratchpad>/verify/gsplice.py <dir with the actual dumps>` after putting those three names into its
   ACCEPT table (the actual dumps are written to `$TMPDIR/az_golden_<group>.actual.txt` by the failing test); any
   OTHER document: do not update, send it to the coordinator. The measured-label test with and without
   `AZ_NO_TAFFY_MEASURE_JOURNAL=1`.) Tell the coordinator rows 1 and 2 at once.
2. (DONE in the gate.) `cargo test -p azul-css --no-fail-fast`, `cargo test -p azul-core` (A8 changed the css parser and core's prop
   cache; `cargo test -p azul-core --lib compact::audit_a8_tests`).
3. (DONE except the fork's own tests, "not run: disk": table "THE GATE" above.) The gate on that tree: `cargo test -p azul-layout --features e2e-server --lib --test all --no-fail-fast`;
   delete its binaries; `cargo test -p azul-dll --lib --no-fail-fast`; `cargo test -p azul-dll --test
   headless_lifecycle`; `cargo check -p azul-dll --target x86_64-unknown-linux-gnu`, then
   `--target x86_64-pc-windows-gnu` (check only; delete `<target>/<triple>` after each). Fork tests only with the
   audit's filters (`render_api::audit_a4_tests`, `spatial_tree`), only if the disk allows, else "not run: disk".
4. The 16 dll failures, the 7 core failures of diff-first's tests and the measured-label test: none is fixed. The coordinator routes them (table above); the two cheap ones a verifier can
   do itself: the `NOT_WALKED` entries of the repointing guard; restating expectations ONLY where an owner's note
   names the change.
5. Milestone 4 to the coordinator with the verified commit; no push. Then, on its word, the pin pass
   (`audit/PIN_PASS.md` in `<scratchpad>/wt/m4p-integration`), building into the editor's target directory, after
   telling the coordinator (it frees space first); the full `azul-doc codegen all` belongs to it.
6. Disk: after the gate the target directory holds no test binary, no `*.rlib` over 3 MB and no `azul-doc`
   binary (deleted for space): the next test build compiles its dependencies again (about 8 minutes for the dll).

## Log

- start: worktree made full (`git sparse-checkout disable`), head `aeb5d0dfe`. Disk 13 GB free. Installed targets:
  aarch64-apple-darwin, x86_64-unknown-linux-gnu, x86_64-pc-windows-gnu (+ wasm32, two bare-metal ones).
- step 1: `cargo update -p agg-rust-azul -p rust-fontconfig`: 1.1.4 -> 1.1.5, 5.0.2 -> 5.1.0 (`42fa49327`;
  `webrender/glyph` asks `^1.0.2` and resolves to the same 1.1.5, its manifest is untouched).
- step 2a: `python3 css/tools/gen_codegen_lowering.py` -> `lower_types.rs` (+4 lines in `API_MODULES`), `5db5f406d`.
- `cargo check -p azul-css`: 0 errors. `cargo check -p azul-core`: 0 errors.
- `cargo check -p azul-layout`: round 1: 5 errors; round 2: 11 errors; all fixed in `4651bf19f`:
  A1 `cpu_backend.rs` (`cpurender::` not imported at module level, E0433 x 4); A1 `raster.rs` / `bands.rs`
  (`SharedShadowCache` made of the private `ShadowCacheEntry`, x 11); A3-window-33 `window.rs`
  (`self.hold_css_dirt(..)` under the borrow of the root's `GpuValueCache`, E0499).
- (builds paused 5 minutes on the coordinator's word; merge round 1 meanwhile.)
- `cargo check -p azul-core`, `-p azul-layout`: 0 errors. `--tests` of css and core: 0 errors.
  `cargo check -p azul-layout --features e2e-server --lib --tests`: 1 error (E0597, `34fc04d1f`), then 0.
- `cargo test -p azul-css`: 2941 passed, 1 failed -> `b87fca8af` -> `--no-fail-fast`: 2942 + 14 targets, 0 failed.
- `cargo test -p azul-core`: 3119 + 30 targets + doc tests, 0 failed.
- run 1, `cargo test -p azul-layout --features e2e-server --lib --test all --no-fail-fast` (head `4f2cdd1f2`):
  lib 10795 passed, 19 failed, 1 ignored (51 s); `all` 3051 passed, 9 failed, 5 ignored (329 s). Triage: the
  sections above. After the fixes the 28 were run one by one: 22 pass, 6 fail (the open table).
- merge round 2 (see "Merged"). `cargo check -p azul-core --tests`: 0 errors;
  `cargo check -p azul-layout --features e2e-server --lib --tests`: 0 errors.
- run 2, the same command (head `0fcd30a97`): lib 10847 passed, 9 failed, 1 ignored (51 s); `all` 3059 passed,
  1 failed, 5 ignored (285 s). Failing: the 6 open ones and 4 new from round 2 (open table, rows 7-10):
  `audit_tests::a2_cpu::a2_cpu_the_damaged_render_is_the_full_render_with_an_overlay_and_without_a_ground`,
  `audit_tests::a2_cpu::a2_cpu_the_damaged_render_is_the_full_render_over_random_pans_zooms_fades_and_edits`,
  `audit_tests::a3_solver::sizing::a_later_turn_of_the_layout_loop_measures_no_label_whose_inputs_stayed`,
  `audit_tests::a3_solver::the_layout_of_a_label_takes_what_its_measure_shaped`,
  `audit_tests::a3_window::a_decoration_written_from_a_callback_is_painted_into_the_list`,
  `audit_tests::a3_window::a_label_moved_where_it_lies_stands_as_a_fresh_layout_puts_it`,
  `audit_tests::a3_window::a_patched_frame_of_a_colour_fade_copies_no_list`,
  `cpurender::raster::audit_a1_tests::a_turned_label_of_a_view_is_drawn_without_a_surface`,
  `cpurender::raster::audit_a1_tests::a_walk_through_the_grids_paints_the_items_of_the_walk_that_tests_every_item`,
  `the_layout_of_a_broad_corpus_matches_its_golden_dumps::the_web_platform_css_fixtures_lay_out_as_their_golden_dump`.
- merge round 3: `audit/A3-solver` at `7f6081e84` (fixes of rows 4, 5, 6(b); 17c `083fe5ada`), `audit/A5` at
  `844d40676`: no conflicts, 0 compile errors (`cargo test ... --no-run`). Targeted run of the rebuilt binaries:
  lib filter `a3_solver floats::audit_a3_solver_tests text_halo image_memo audit_tests::a5`: 150 passed, 0 failed
  (rows 4 and 5 are FIXED: `7e46a8945`, `64b2e4fc9`); `all` filter `the_layout_of_a_broad_corpus`: 6 passed, 0
  failed: `the_web_platform_css_fixtures_lay_out_as_their_golden_dump` passes against the goldens as they are, so
  row 6(b) is FIXED by `7f6081e84` (the ten floats are on one line again) and the three `vertical-align` documents
  read as their old golden again (96 px line box, `fs=1`): row 6(a) is no longer red, the defect it showed
  (`font-size: 1in` reaches the glyphs as 1 px) is still there and with the text owner.
- `cargo build -p azul-doc` (dev profile): 0 errors (1 min 38 s). Codegen: see "Generated code".
- merge round 4 (see "Merged"): no conflicts. `cargo test -p azul-layout ... --no-run`: 0 errors.
  Groups run from the rebuilt lib binary (before A3-window `022638f7e`): `cpurender audit_tests::a2_cpu
  audit_tests::a10 audit_tests::a1 widgets::node_graph cpu_backend e2e::cpu_view`: 1061 passed, 4 failed:
  two compositor expectations moved by A1-64 (corrected, `Tests changed`), and the two random a2_cpu tests,
  which stopped in the harness line of `Stage::write` (a veil write answered `Unchanged` for a value that was
  not the last written; `022638f7e`'s `91ca03104` fixes a "before" read after the inline write: run again with it).
  Rows 3 and 10 passed in that run (FIXED by A1-63 `978eccb67` and A1-65).
- **azul-dll compiles**: `cargo check -p azul-dll` 0 errors (webrender fork and macOS shell included);
  `cargo check -p azul-dll --lib --tests` 0 errors; `--no-default-features --features link-static,serde-json,png`
  0 errors; the same + `e2e-scripting` 0 errors. No fix was needed in the dll.
- run 3, `cargo test -p azul-layout --features e2e-server --lib --test all --no-fail-fast` (head `57a46db75` =
  merge round 4 + `audit/A3-window` `d68713628`): lib 10867 passed, 2 failed, 1 ignored (49 s); `all` 3059 passed,
  1 failed, 5 ignored (282 s). Failing:
  `audit_tests::a2_cpu::a2_cpu_the_damaged_render_is_the_full_render_over_random_pans_zooms_fades_and_edits`,
  `audit_tests::a2_cpu::a2_cpu_the_damaged_render_is_the_full_render_with_an_overlay_and_without_a_ground` (both in
  my harness line: a first write "veil at 80 %" is `Unchanged`, the veil rests at 0.8: harness corrected in
  `8193ae482`, to be run again), and NEW, pre-existing test
  `a_measured_label_flows_its_text_only_at_a_new_width::a_hovered_label_flows_its_text_once_however_often_it_is_measured`
  ("the hover flowed text 3 times for one label laid out at a new width after 2 measures: a measure took the place
  of the label's final layout"; passed in run 2; candidates: A3-solver 17c `083fe5ada`, `7e46a8945`).
  Rows 7, 8, 9 pass (FIXED by `4856559a5`, `91ca03104`, `022638f7e`); rows 3 and 10 pass.
- `cargo test -p azul-dll --lib --no-fail-fast` (same head, default features): **3834 passed, 16 failed, 9
  ignored** (69 s). The 16 with their messages: `<scratchpad>/verify/31-failures.txt`; table "dll lib failures"
  above. Disk fell to 8.2 GB during this build (under the floor); test binaries and rlibs deleted after.
- (the session was stopped by the usage limit here and resumed.) CHECKPOINT plan of the coordinator: merge only
  `audit/A8` at `1b3cd3449` (done: `b419ea882`, no conflict), rows 1 and 2, then the gate on that tree; the rest
  of the queue is a second round: `audit/A1` at `e1494255d` (text ink table), `audit/A3-solver` at `33a88a9ed`,
  `audit/A5` at `dbde1ccf5`, newer `audit/A3-window`, `audit/diff-first` beyond `cfc4d6d72`, `audit/dom-builder`.
- the gate on the checkpoint tree (head `e19bed682`): see the table "THE GATE". Linux and Windows checks: 0
  errors each. Fork tests: not run (disk: 9.6 GB free after the gate; the fork's test build is a full build of
  webrender's dependencies). Compile fixes needed by the whole branch: 17, all in azul-layout (none in css, core,
  dll, the fork, or for Linux / Windows).
- the verifier stops here (context short). Everything is committed; nothing was pushed.

### Second verifier (took over at `a23a72fde`; same rules, same environment)

- step 1: `2f34d89fa`: the repointing guard's `NOT_WALKED` lists the six audit fixtures (`a5.rs Recorder`,
  `a10/layer.rs Window`, `a2_cpu.rs Stage`, `a3_solver/raised_bars.rs ViewWindow`, `a3_window.rs WatchedWindow`,
  `a6.rs Scene`; const `FIXTURE`). All six were read: each is a struct of a test module.
- step 2: merge `audit/A4` at `a1fc02edf` (`d6ff63ad7`, no conflict). `cargo test -p azul-dll --lib --no-run`: 0
  errors (log `50-build-dll-tests`). From the test binary, filters `audit_tests::a4 audit_tests::a2_gpu
  retained_list_tests every_holder_of_a_nodes_data frame_schedule` (log `51-dll-a4`): 36 passed, 2 failed. FIXED:
  `retained_list_tests::a_pan_inside_the_margin_...`, `::leaving_the_margin_...` (A4-38 `11d6afef6`),
  `a4_31_a_settle_sent_without_the_wait_...` (`f1805b5a9`), `a2_gpu::a_panned_views_content_reaches_webrender_...`
  (`a816b1ae0`), the 1x halo twin (removed with its reason, `bf3955421`; row A4-39 open, low), the repointing guard.
  Still failing there (A3-window's two restatements, not merged at that point):
  `retained_list_tests::a_view_the_frame_does_not_send_again_is_still_drawn`,
  `::the_hover_of_a_label_in_the_map_sends_the_maps_list_alone_and_draws_it`.
- step 3 (three of four tips, merged together, one build): `audit/A3-solver` at `79a700d1f` (`1fc01c22b`),
  `audit/A1` at `6c4d7265a` (`45720ab97`), `audit/A3-window` at `1af12829a` (`f1e2af9c8`). One conflict, A1 x
  A3-window in `solver3/display_list.rs` `try_copy_cached_run`: both kept (A1's `builder.next_text_ink = ..` line,
  then A3-window's `builder.next_color_binding = binding;`). `audit/diff-first` is still to come (the coordinator
  names the commit; a second conflict in the same function is expected, its commit message says how the lines read).
- compile fix `b1a7898b2`: `DisplayListGenerator::new` was a `const fn`; A3-core-26's `glyph_boxes` is a `HashMap`
  (E0015). The only error of the three branches together (the ink table included).
- layout lib, whole suite, on `b1a7898b2` (log `53-layout-lib`): **10897 passed, 1 failed, 1 ignored**. Failing:
  `audit_tests::a3_window::a_pointer_swept_over_labels_it_never_touched_writes_values_and_changes_no_list`
  (harness; corrected in `270e178c9`, see "Tests changed"; to be confirmed by the next layout build). Named and
  passing: A3-solver `a_journaled_measure` (2), `a_measure_of_` (3), `a_measure_is_kept`, `retain_content`,
  `node_graph` (146); A4 `a_list_that_is_only_named_by_weak_references_is_written_in_place`; A1 the eight ink
  tests, the hull / spinner-arc tests, `gpu_value_damage_*` (6), `a2_cpu_04_*`, both random
  `a2_cpu_the_damaged_render_is_the_full_render_*`, `cpurender` 871 / 0 / 1 ignored; A3-window 53 of 54.
  `a_box_that_moved_in_a_pass_is_painted_as_a_fresh_layout_paints_it` passes WITHOUT diff-first's fix: after the
  diff-first merge run it once with `AZ_DL_TRANSLATE_MOVED_RUNS=1` and tell the coordinator whether it fails
  (a real guard) or still passes (its fixture is then corrected in the second round).
