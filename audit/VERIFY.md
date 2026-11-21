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
| azul-layout | yes (default features and `--features e2e-server`) | yes (`--lib --tests`) | run 1 (before round 2's merges): lib 10795 passed / 19 failed / 1 ignored; `tests/all` 3051 passed / 9 failed / 5 ignored. 22 of the 28 settled, 6 open (below). Run 2 (after round 2's merges): see the log |
| azul-dll | not yet (needs `target/codegen`: `azul-doc codegen all`) | | |

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
- NOT merged (the coordinator's word is needed): `audit/diff-first` beyond `cfc4d6d72`, `audit/A3-window` beyond
  `257052358`, `audit/dom-builder`.

Generated code: `css/src/codegen/lower_types.rs` is current (`python3 css/tools/gen_codegen_lowering.py`: no change
after round 2). `target/codegen` (the dll's bindings) is NOT made yet: `cargo build -p azul-doc` then
`<target>/debug/azul-doc codegen all` from the worktree root (it writes `<worktree>/target/codegen`; `dll/build.rs`
finds it there; of its about 940 MB only `*.rs`, `api.json.br` and `material_icons.ttf.br` are needed).

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

Other things found, routed by the coordinator:
- A node placed by a per-node `Dom::with_css` string (the editor's tiles and symbols, `map.rs` 673-808) does not
  get A3-solver-41's inset path; only `with_css_props` nodes do.

## `[reveals]` tests

Not decided yet (each needs its fix reverted in a scratch build to see the test fail without it): A3-solver-32
(`d9e4f6da1` / fix `1e8fd3a32`), -44 (`9943af97e` / `88ad0aa7b` + `e193ca69f`), -45 (`be2c9aff1` / `aeb67d400`), -46
(`318ecf506` / `a3ddc3b26`), -52 (`5a31c9af8` / `d14e782a9`), -53 (`6f54a959e` / `9019db74d`), -54 (`5c0073540` /
`6f402e476`). All seven pass WITH their fixes (run 1). A3-solver-32's fix moved exactly the golden lines its
commit predicted, so that bug was real.

## Not done yet / next

1. Run 2 of the layout suites (after round 2's merges): running. Then write its totals and failing names here.
2. `azul-doc` build + `codegen all`; `cargo check -p azul-dll` (default features; then
   `--no-default-features --features link-static,serde-json,png`, the editor's set; then with `e2e-scripting`).
3. `cargo test -p azul-dll --lib`, `--test headless_lifecycle`.
4. `cargo check -p azul-dll --target x86_64-unknown-linux-gnu`, `--target x86_64-pc-windows-gnu` (both installed).
5. The webrender fork's own tests (A4 changed the fork; "the fork's tests run with the fork's own test command").

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
- run 2 of the layout suites: started (head `0fcd30a97`).
