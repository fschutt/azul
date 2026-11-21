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
| azul-css | not yet | | | |
| azul-core | not yet | | | |
| azul-layout | not yet | | | |
| azul-dll | not yet | | | |

In progress: step 1 (dependencies). Next: css check.

## Reverted commits

(none yet)

## Tests changed by the verifier

(none yet)

## Open (what still fails / needs a decision)

(none yet)

## Log

- start: worktree made full (`git sparse-checkout disable`), head `aeb5d0dfe` (merges of A3-solver, A1, A6, A4 on top).
  Disk 13 GB free. Installed targets: aarch64-apple-darwin, x86_64-unknown-linux-gnu, x86_64-pc-windows-gnu (+ wasm32,
  two bare-metal ones).
- step 1: `cargo update -p agg-rust-azul -p rust-fontconfig`: agg-rust-azul 1.1.4 -> 1.1.5, rust-fontconfig 5.0.2 -> 5.1.0
  (`Cargo.lock`; `webrender/glyph` asks `^1.0.2` and resolves to the same 1.1.5, its manifest is untouched).
