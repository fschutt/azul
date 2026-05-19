# Next-agent prompt — M12.5d cascade-blocker

Continue M12: get `StyledDom::create` to return real internals when
lifted AArch64→wasm. Read `scripts/HANDOFF_2026_05_20_M12_5d.md`
first (full context) and the memory note
`m12_cascade_neon_blocker.md`.

Branch `layout-debug-clean`, HEAD `4e302892a`. All M11 work landed;
M11 gates GREEN. The cascade's heap output at `styled_ptr` is
all-zero — that is THE blocker.

## The one fact that matters

Bisect proved: **merely LIFTING a function whose signature has a
`&mut T` arg corrupts the const-pool reads of OTHER lifted
functions** (build-time, not runtime). By-value args don't trigger
it. `StyledDom::create(dom: &mut Dom, css) -> Self` has `&mut Dom`,
so its whole transitive chain poisons the build. The `strip_noalias`
fix already in the tree did NOT fix it — `&mut` lowers to more
attributes than just `noalias`.

## Deterministic reproducer (use this, don't re-derive)

1. `make_test_struct()` heap = 64/64 correct at clean HEAD.
2. Add `#[inline(never)] #[no_mangle] fn f(x: &mut u32) ->
   TestStruct256` to eventloop.rs.
3. In hydrate add `if black_box(state)==0xDEADBEEF {
   Box::new(f(&mut a)); }` (always-false; lifted, never run).
4. Rebuild → `make_test_struct` heap is now corrupted.
   Remove f → heals.
   **TEST ONLY ONE reproducer at a time** — stacking many Box::new
   test allocs traps hydrate (a prior misdiagnosis came from this).

## Do this

1. Diff the lifted/opt IR (or wasm `.wat`) for `make_test_struct`
   between the +&mut-fn build and the clean build. Find what
   changes about its emitted const-pool loads.
2. Extend `strip_noalias_from_sub_args` to also strip
   `dereferenceable`, `align`, `nonnull`, `readonly`, `writeonly`
   from `@sub_*` args; rebuild + reproducer. If it heals,
   that's the fix.
3. If attributes aren't it, suspect the mirror: the &mut-fn's
   adrp+ldr ranges may displace make_test_struct's in
   `build_mirror_segments` zero-trim/page-merge. Re-add the
   `AZ_DEBUG_MIRROR_ALL` eprintln and compare ranges.
4. Once cascade returns ≥1 node, proceed to Phase 5: lift
   `LayoutWindow::layout_dom_recursive` for real positioned_rects.

## Hard rules (from user)

- `gh` cli (logged in as fschutt). NO hacks — upstream-acceptable
  remill fixes. Don't commit the parallel iOS/Android agent's
  files — only stage files you edited. On cargo conflicts, wait +
  retry. Commit each meaningful step with
  `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`.
- **Disk**: `rm -rf /var/folders/*/T/azul-web-transpiler-*` before
  any `AZ_REMILL_KEEP_SCRATCH=1` run; they fill the disk fast.

## Build + test

```sh
cargo build -p azul-dll --release --features build-dll,web-transpiler-static
AZ_BACKEND=web://127.0.0.1:8800 ./examples/c/hello-world-v5.bin &
sleep 20
node scripts/m9_e2e/styled-dom-hydrate.js   # [7] should show node_data.len() ≥ 1 when fixed
```
