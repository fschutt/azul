# M12.5x Handoff — 2026-05-20 — Cascade blocker root-caused to a self(X0)-relative store mis-lift

## TL;DR

- **Root cause #1 — FIXED & SHIPPED** (keeper commit `999f2abdc`): `alloc::raw_vec::grow_one`
  was classified `Leaf` → noop-stubbed, so lifted `Vec`s never grew and the whole cascade
  output was all-zero. Fix: classify `alloc::raw_vec` as `Recursable` so it is lifted. The
  cascade now *executes*.
- **Root cause #2 — ROOT-CAUSED, NOT yet fixed.** After the cascade runs, its output is still
  corrupt. We proved this is a **remill register *value*-tracking mis-lift**: in lifted
  `CssPropertyCache::apply_ua_css`, the `self.cascaded_props.push_to(...)` element-copy writes
  to **`self+0` (node_count)** instead of the heap slot — because a register holds the cache
  pointer (`self`/X0) at runtime when it should hold a heap pointer. node_count goes `1 → 0`,
  which surfaces downstream as the 768 MiB OOB / "node_count read as pointer" trap.
- The bug is **invisible to static IR analysis** (the lifted IR is *structurally* correct) and
  **Heisenbug-defeated at runtime** (every probe we add shifts register allocation and moves the
  corruption). The autonomous build-probe loop cannot crack it; it needs **remill-internals work**.
- **Backups pushed** (this session's 18 commits + the remill fork work) — see "Backups" below.

## Backups (pushed 2026-05-20)

azul (`github.com/fschutt/azul`):
- `backup/layout-debug-clean-m12-cascade-investigation-20260520-093136` @ `31a269037` — full
  investigation (HEAD; all 18 session commits + debug scaffolding).
- `backup/layout-debug-clean-m12-grow-one-keeperfix-20260520-093136` @ `999f2abdc` — the keeper
  grow_one fix milestone (cascade first *runs*, before heavy debug scaffolding piled on).

remill fork (`github.com/fschutt/remill`, branch `m12-q-reg-x8-sret`, submodule `third_party/remill`):
- `m12-q-reg-x8-sret` @ `212d3e4` — fast-forwarded +2 (LD1 single-elem; ST2/ST3/ST4 + LD3/LD4
  post-index + ADDP). NO PR/MR opened (per instruction).
- `backup/m12-q-reg-x8-sret-20260520-093254` @ `212d3e4` — pinned backup.

## Root cause #2 — the full narrowing (verified, do NOT re-derive)

Diagnostic harness: a `#[no_mangle] static mut AZ_DBG_NC: [u64; 48]` in
`core/src/compact_cache_builder.rs` + getter `AzStartup_getDbgNc(i)` (eventloop.rs / mod.rs export
allowlist / transpiler_remill.rs CallbackSignature peekU32 arm). Native-safe (raw fixed-address
writes crash the server's native cascade pre-render; the static-mut does not). Read via
`scripts/m9_e2e/baseline-probe.js`.

1. **5-point node_count bracket** in `create_from_compact_dom` (styled_dom.rs): `[7]` after
   `empty()` = 1, `[8]` after `restyle()` = 1, **`[9]` after `apply_ua_css()` = 0**, `[10]`/`[11]` = 0.
   → `CssPropertyCache::apply_ua_css` (prop_cache.rs ~4977) is the corrupting op.
2. **Within-apply_ua_css bracket** (commits `23a55416a`, `6daaeaa80`): drop(css), the prop_set
   `vec![[0u128;2];n]` build, and `get_ua_property` are all INNOCENT; the corruption is the
   **single `self.cascaded_props.push_to(node_index, StatefulCssProperty{..})`** (prop_cache.rs ~5081).
3. **self is STACK-resident**: `self`(X0) = wasm linear `0x2ee10`, below the heap base `0x6000000`
   (bump_ptr `0x60022a8`). The cache lives on the mini module's relocated stack (SP base `0x30000`).
   The whole cascade is **intra-mini** (133 lifted fns; the on_click/layout callbacks are separate
   modules at SP `0x50000`/`0x70000`).
4. **PADDING TEST = the decisive proof (commit `31a269037`, M12.5x)**: added an 8 KB
   `core::hint::black_box([0xAB;8192])` local before `css_property_cache` in `create_from`. Result:
   apply_ua_css's SP moved **down 8 KB** (`0x2ebc0 → 0x2cbc0`) but **self stayed `0x2ee10`** and
   **`[9]` was still 0**. A fixed `SP+offset` store would now miss self; it still hits self →
   **the corrupting store is self(X0)-relative, not SP-relative.**
5. **Static IR confirms the structure is correct** (M12.5z): the threaded apply_ua_css =
   `sub_a48664` in dep `__az_dep_<slid>.opt.ll`. The push element-copy slot is
   `slot = idx*144 + %v.i828`, where `%v.i828 = *(&inner_vec + 8)` = grow_one's heap allocation via
   the proper `self+0x28 → build.ptr → &inner_vec → inner.ptr` chain. **No dropped offset, no wrong
   base in the IR.** So a register holds self's value at *runtime* despite correct SSA → a remill
   register value-tracking mis-lift. The element copy's first store (slot+0) writes the
   StatefulCssProperty's first 8 bytes `{state=Normal=0, prop_type=Display=0}` = 0 over node_count.

## Disproven this session (do NOT repeat these)

- **NEON-Q PAIR lift (STP_Q/LDP_Q)** — M12.5r: rebuilt with `RUSTFLAGS=-C llvm-args=-aarch64-enable-ldst-opt=0`
  (verified the 144 B copy became single `str q`/`ldr q`, no pairs). Cascade STILL broke (`[9]=0`).
  → pair-instruction lift is NOT the bug.
- **inline u128** (`1u128<<d`, `[u128;2] |=`) — `make_test_u128` reproducer reads back perfectly.
- **grow_one stack handling** — lifted IR (dep `10b96d2b0`) verified: prologue/spills/writeback to
  `*X19`(heap)/epilogue all correct, SP balanced.
- **apply_ua_css frame size / frame overlap** — frame is 416 B, all SP-relative offsets in-frame;
  self−SP = 592 (and 8784 with padding) > frame → no overlap.
- **~11 reproducers ALL lift cleanly** (std Vec, multi-Vec, AzVec from_vec, Vec<large droppable>,
  recursion+&mut-Vec-accum, FlatVecVec-shaped, u128, sret structs). Reproducers are a different
  codegen scope than apply_ua_css → they do NOT reproduce. **STOP building reproducers.**
- mirror (AZ_FORCE_WHOLE_PAGE), SROA, opt level (O0 same), state-sharing — all ruled out earlier.

## Why the loop can't crack it (the Heisenbug)

The corruption is sensitive to register allocation. Every observation we add changes it:
- `build_get(node_index)` before the push → corruption *vanishes* (push#1 clean).
- `#[inline(never)]` on `push_to` → 24 pushes all clean, corruption migrates *post-loop*.
- the cache-byte dump's own `p.add(k)` is itself lifted by the buggy transpiler → read all-zero.
- `sp_probe` (minimal) *survives*; bigger probes don't.
3 distinct manifestations observed (node_count→pointer / →0 / whole-cache→0). Static IR shows correct
structure. **Conclusion: the only Heisenbug-proof path is to instrument the lift itself.**

## Recommended next steps (priority order)

1. **Instrument the lift to log store addresses (THE path).** In
   `dll/src/web/transpiler_remill.rs`, the per-dep pipeline is: remill-lift → `.patched.ll` →
   `opt -O2` → `.opt.ll` → `llc -mtriple=wasm32` → `.o` → wasm-ld. Add a debug mode (env, e.g.
   `AZ_LOG_STORES=<fn>`) that, for the target dep's `.opt.ll`, injects before each `store ..., ptr %p`
   a call to a logging helper recording the i32 address + a per-store id IF the address is in the
   stack page (e.g. `0x2e000–0x2f000`, or `> current SP`). Rebuild → run → the logged id pinpoints
   the corrupting store. Then trace which AArch64 instruction lifted to it.
2. **Or fix remill's AArch64 register tracking directly.** The bug is a register holding X0(self)'s
   value at a load/store where a heap pointer (inner.ptr / build.ptr / X27=&inner_vec) was expected.
   Candidate native region (instrumented build, vmaddr `0x93880c`, region `0x938e74–0x938ee4`): after
   `bl grow_one`, `ldr x8,[x27,#0x8]` (inner.ptr reload) → `madd x8,x21,#0x90,x8` (slot) → the NEON
   `ldp/stp q` 144 B copy. Inspect how remill's fork lifts the post-call register reload / the
   address-forming `madd`, looking for a value-tracking slip where X0/self leaks into x8/x27.
3. **Verify the fix**: rebuild, run, `node scripts/m9_e2e/baseline-probe.js` → `[9]` after
   apply_ua_css should be **1** (was 0), node_data ptr=heap & len=1, no 768 MiB trap,
   `AzStartup_getStyledDomNodeCount` sane.
4. **At a clean cascade-working milestone**, REVERT all debug scaffolding (see "Scaffolding to
   revert") keeping ONLY the keeper fix `999f2abdc` + the real remill/transpiler fix. Commit
   "cascade WORKING milestone". Then proceed to **M12.6**: lift `LayoutWindow::layout_dom_recursive`.

## Scaffolding to revert at the clean milestone (all currently committed in the session branch)

- `core/src/compact_cache_builder.rs`: `AZ_DBG_NC` (revert `[48]`→remove), its writes.
- `dll/src/web/eventloop.rs`: `AzStartup_getDbgNc`, `make_test_*` reproducers, the hand-built-Dom
  substitution + reproducer Box::new/poke block in `AzStartup_hydrate`/`hydrateStyledDom`,
  BumpAlloc size/count instrumentation.
- `dll/src/web/mod.rs` + `transpiler_remill.rs`: the `AzStartup_getDbgNc` export wiring.
- `core/src/styled_dom.rs`: the node_count bracket captures, the 160 B cache dump, the `m12_pad`
  black_box local (line ~1045).
- `core/src/prop_cache.rs`: the within-apply_ua_css captures (`AZ_DBG_NC[12..15]`, sp_probe,
  per-push counter). `push_to` is already back to `#[inline]`.
- `scripts/m9_e2e/baseline-probe.js`: all the M12.5* probe blocks (keep the harness, drop the dead ones).
- KEEP: `999f2abdc` (symbol_table.rs `alloc::raw_vec` → Recursable).

## Test commands

```sh
# Build (release; web transpiler statically linked):
cargo build -p azul-dll --release --features build-dll,web-transpiler-static > /tmp/build.log 2>&1
echo "CARGO_EXIT=$?"               # check this, do NOT pipe to tail (swallows exit code)
cp -f target/release/libazul.dylib target/release/deps/libazul.dylib   # the .bin loads deps/

# Run server (keep scratch for IR inspection):
pkill -f hello-world-v5.bin; sleep 1
AZ_REMILL_KEEP_SCRATCH=1 AZ_WASM_DEBUG=1 AZ_BACKEND=web://127.0.0.1:8800 \
  ./examples/c/hello-world-v5.bin > /tmp/server.log 2>&1 &
# poll http://127.0.0.1:8800/ (~18 s for lift), then:
node scripts/m9_e2e/baseline-probe.js     # read the node_count BRACKET: [9] == 1 when fixed

# Recompute the ASLR slide to find apply_ua_css's IR dep (per-process):
#   deps = ls <scratch>/__az_dep_*.opt.ll ; find S so apply_ua_css(nm CssPropertyCache12apply_ua_css)+S
#   AND get_ua_property(0x9ac8e4-ish)+S are BOTH in deps. Threaded fn = the sub_<addr>, NOT the wrapper.

# Disk (builds + scratch are large): rm -rf target/debug (unused, ~3 G); scratch dirs for DEAD pids
#   only: for d in /var/folders/*/*/T/azul-web-transpiler-*; do pid=${d##*-}; kill -0 $pid 2>/dev/null || rm -rf $d; done
```

## Key files

- `core/src/prop_cache.rs` — `CssPropertyCache::apply_ua_css` (~4977), `FlatVecVec::push_to` (~424),
  struct layout: `node_count@0`, `user_overridden@8`, `cascaded_props(FlatVecVec)@0x20`.
- `core/src/styled_dom.rs` — `create_from_compact_dom` (~1020), the bracket captures.
- `dll/src/web/transpiler_remill.rs` — the lift pipeline (`symbol_table.rs` `classify_for_name` →
  the keeper fix is in symbol_table.rs); per-dep `opt`/`llc` pipeline ~lines 770–1010; stack
  relocation `relocate_stack_if_non_mini` (~2125, STACK_BASE 192 KiB / stride 128 KiB).
- `dll/src/web/symbol_table.rs` (~1757) — **the keeper fix** (`alloc::raw_vec` → Recursable).
- `scripts/m9_e2e/baseline-probe.js` — the probe.

## Memory note

`~/.claude/.../memory/m12_cascade_neon_blocker.md` (sections M12.5e..M12.5z) + MEMORY.md index line
(M12.5z CONCLUSIVE). Full reasoning trail, every dead-end with evidence.
