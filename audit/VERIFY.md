# Verification pass of `audit/integration` (verifier, 2026-11-21)

The one pass of the audit that compiles and runs. Everything on this branch below `aeb5d0dfe` was written blind
(about 520 commits, 193 files, +72,000 lines). This file is the log: each command, its result, every commit the
verifier made, every commit it reverted and why. Newest state first, then the chronological log.

Environment of every build: `~/Development/mini-cc/tools/cg cargo <...> -j 2`, one at a time,
`CARGO_TARGET_DIR=<scratchpad>/azul-target`, `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_{DEV,TEST,RELEASE}_DEBUG=0`,
rustc 1.91.0 (the pinned toolchain), macOS aarch64. Logs: `<scratchpad>/verify/<nn>-<name>.log` (not committed).

## State (kept current)

| crate | compiles | tests compile | tests | notes |
|---|---|---|---|---|
| azul-css | yes | yes | `cargo test -p azul-css --no-fail-fast`: lib 2942 passed, 14 integration targets passed, 0 failed | 1 test corrected (`66` below) |
| azul-core | yes | yes | `cargo test -p azul-core`: lib 3119 passed, 30 integration targets and the doc tests passed, 0 failed | |
| azul-layout | yes (default features, and `--features e2e-server`) | yes (`--lib --tests`, 1 error fixed) | running | 17 errors fixed in all |
| azul-dll | not yet (needs `target/codegen`: `azul-doc codegen all`) | | | |

In progress: step 4, `cargo test -p azul-layout --features e2e-server --lib --test all --no-fail-fast`.
Next: the failures of that run; then `azul-doc codegen all` and azul-dll.

The gate of the base (`e02646d9a`; from the previous session's `gate-b3.sh` and its logs, the numbers of the commit
message were attached to the wrong suites):
- `cargo test -p azul-layout --features e2e-server --lib --test all`: lib 10404 passed, 1 ignored; `all` 3060 passed, 5 ignored
- `cargo test -p azul-dll --lib` (default features, `AZ_CODEGEN_DIR` = generated sources): 3731 passed, 9 ignored
- `cargo test -p azul-dll --test headless_lifecycle`: 8 passed
- `cargo check -p azul-dll --target x86_64-unknown-linux-gnu` and `--target x86_64-pc-windows-gnu`

## Reverted commits

(none yet)

## Tests changed by the verifier

- `css/src/props/style/box_shadow.rs` `scale_for_dpi_quantizes_below_the_fixed_point_precision` (commit after
  `f4519ce3a`'s fix, subject "a css test pinned FloatValue's truncation ..."): it scaled 8 px by 0.0001 and asked 0;
  0.0008 truncated to 0 and rounds to 0.001 since A3-core-14 (`fb8751e41`, `FloatValue::new` rounds, on purpose; its
  commit says a value computed at run time escapes its literal scan). Now: factor 0.00005 -> all zero, factor 0.0001
  -> `[0, 0, 0, 0.001]`. Code unchanged.

## Open (what still fails / needs a decision)

(none yet)

## Log

- start: worktree made full (`git sparse-checkout disable`), head `aeb5d0dfe` (merges of A3-solver, A1, A6, A4 on top).
  Disk 13 GB free. Installed targets: aarch64-apple-darwin, x86_64-unknown-linux-gnu, x86_64-pc-windows-gnu (+ wasm32,
  two bare-metal ones).
- step 1: `cargo update -p agg-rust-azul -p rust-fontconfig`: agg-rust-azul 1.1.4 -> 1.1.5, rust-fontconfig 5.0.2 -> 5.1.0
  (`Cargo.lock`; `webrender/glyph` asks `^1.0.2` and resolves to the same 1.1.5, its manifest is untouched).
- step 2a: `python3 css/tools/gen_codegen_lowering.py` -> `css/src/codegen/lower_types.rs` (+4 lines in `API_MODULES`;
  the tool's cross-check of api.json against `css/src` passes). Commit `5db5f406d`. Run again after the merge of A6's
  `e60317650` (two `StyledDom` fields): no change.
- `cargo check -p azul-css`: 0 errors (4 warnings). `cargo check -p azul-core`: 0 errors (4 warnings).
- `cargo check -p azul-layout` (default features), round 1: 5 errors. Round 2 (after the fix of round 1): 11 errors.
  All fixed in `4651bf19f`:
  - A1 `cpu_backend.rs`: `cpurender::` used at module level, imported only inside `render_frame` (E0433 x 4);
  - A1 `raster.rs` / `bands.rs`: `SharedShadowCache` (pub(crate)) is made of the private `ShadowCacheEntry`
    ("type is private" x 11): struct and alias made `pub(crate)`;
  - A3-window-33 `window.rs` (the animation tick): `self.hold_css_dirt(..)` while the root's `GpuValueCache` is
    borrowed (E0499): the body became an associated function over the field `css_dirty_waiting`.
- coordinator: builds paused (the build slot is lent). Merged meanwhile, no conflicts: `audit/A6` @30f902802
  (A6-16b / A6-18b `e60317650`: `StyledDom::focusable_nodes`, `nodes_with_markers`; api.json +6 lines), `audit/A1`
  @bf57ffc9a (A1-58/-59 conic gradients, A1-60), `audit/A10` @7d549eebe (notes only). `audit/A3-solver`,
  `audit/A3-window`, `audit/A4` hold only merges of integration (nothing to take). `audit/diff-first` not merged
  (waits for the coordinator's word).
- builds resumed (coordinator: "continue"). Merged, no conflicts: `audit/A1` @f2ecfda91 (A1-47 borders), `audit/A4`
  @8a7af63ed (notes), `audit/A3-window` @914e54179 and `audit/A3-solver` @ceb7587af (no tree change). Head `ee64815dc`.
- `cargo check -p azul-core`: 0 errors. `cargo check -p azul-layout`: 0 errors (93 warnings). **css, core and layout
  libs compile.**
- `cargo check -p azul-css --tests`: 0 errors. `cargo check -p azul-core --tests`: 0 errors.
  `cargo check -p azul-layout --features e2e-server --lib --tests`: 1 error (E0597 in a test helper of
  `text3/cache/unified_layout.rs`, A8-23's test module: the tail expression's iterator outlived the local), fixed;
  second run 0 errors.
- `cargo test -p azul-css`: lib 2941 passed, 1 failed (`scale_for_dpi_quantizes_below_the_fixed_point_precision`: see
  "Tests changed"); after the correction `cargo test -p azul-css --no-fail-fast`: 2942 + 14 integration targets, 0 failed.
- `cargo test -p azul-core`: lib 3119 passed, every integration target and the doc tests passed, 0 failed.
- `cargo test -p azul-layout --features e2e-server --lib --test all --no-fail-fast`: started.
