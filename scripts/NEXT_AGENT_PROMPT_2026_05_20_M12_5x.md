# Next-agent prompt — M12.5x cascade blocker (2026-05-20)

Copy the block below as the opening prompt for the fresh agent. It is self-contained; the agent
should READ `scripts/HANDOFF_2026_05_20_M12_5x_self_relative.md` and the memory note
`m12_cascade_neon_blocker.md` first.

---

You are continuing the azul **web backend** cascade blocker. Yesterday's session root-caused it;
your job is to **FIX it**, armed with those findings — do not re-derive. READ FIRST:
`scripts/HANDOFF_2026_05_20_M12_5x_self_relative.md` (full detail + test commands + backups) and the
memory note `m12_cascade_neon_blocker.md`. Current branch `layout-debug-clean`; full investigation is
backed up at `backup/layout-debug-clean-m12-cascade-investigation-20260520-093136`, keeper-fix
milestone at `backup/layout-debug-clean-m12-grow-one-keeperfix-20260520-093136`; remill fork work is on
`m12-q-reg-x8-sret` (submodule `third_party/remill`).

CONTEXT: the web backend lifts AArch64 machine code from `libazul.dylib` to WebAssembly via a
remill-based transpiler (`dll/src/web/transpiler_remill.rs`) at server startup. The CSS cascade runs
inside the lifted wasm.

STATE:
- ROOT CAUSE #1 FIXED & SHIPPED (keeper commit `999f2abdc`): `alloc::raw_vec::grow_one` was
  `Leaf`-stubbed → Vecs never grew → all-zero cascade. Now `Recursable` (lifted). Cascade RUNS.
- ROOT CAUSE #2 (your target): in lifted `CssPropertyCache::apply_ua_css`, the single
  `self.cascaded_props.push_to(...)` element-copy writes `0` to **node_count @ self+0** instead of the
  heap slot, because a register holds the cache pointer `self`(X0) at RUNTIME where it should hold a
  heap pointer. PROVEN self(X0)-relative (not SP-relative) by the padding test. The lifted IR is
  *structurally correct* (slot = idx*144 + inner.ptr via the proper self+0x28→build.ptr chain) — so
  this is a **remill register VALUE-tracking mis-lift**, not a structural one.

DO NOT REPEAT (all disproven with evidence — see handoff): NEON-Q pair lift (disabled, still broke),
inline u128, grow_one stack handling, apply_ua_css frame size/overlap, ~11 reproducers (all lift
clean — reproducers are a different codegen scope, they will NOT reproduce; STOP building them),
mirror/SROA/opt-level. Runtime address-observation is HEISENBUG-DEFEATED (every probe shifts register
allocation and moves the corruption). Quick static IR tracing shows correct structure.

THE PATH (Heisenbug-proof) — instrument the LIFT, or fix remill directly:
1. Add a debug mode to `dll/src/web/transpiler_remill.rs` that injects, into the target dep's
   `.opt.ll` (threaded `apply_ua_css` = `sub_<addr>`, found via the ASLR-slide dep-match in the
   handoff), a logging call before each `store ..., ptr %p` that records the i32 address + a per-store
   id when the address is in the stack page (`> SP` / `0x2e000–0x2f000`). Rebuild, run, read the log:
   the id pinpoints the corrupting store. Then map it back to the AArch64 instruction.
2. OR inspect remill's AArch64 lift of the push region (instrumented build vmaddr `0x93880c`, region
   `0x938e74–0x938ee4`: after `bl grow_one`, `ldr x8,[x27,#0x8]` reload → `madd x8,x21,#0x90,x8` slot
   → NEON `ldp/stp q` 144 B copy) for a value-tracking slip where X0/self leaks into x8/x27/the slot
   base. Fix in `third_party/remill` (branch `m12-q-reg-x8-sret`) — upstream-acceptable, NO source
   workarounds, and do NOT open a PR/MR against remill (push to the fork branch only).
3. VERIFY: rebuild; `node scripts/m9_e2e/baseline-probe.js`; the node_count BRACKET `[9]` (after
   apply_ua_css) must be **1** (currently 0); node_data ptr=heap & len=1; no 768 MiB OOB trap.
4. THEN revert all debug scaffolding (handoff lists it) keeping ONLY `999f2abdc` + the real fix; commit
   "cascade WORKING milestone"; proceed to M12.6 (lift `LayoutWindow::layout_dom_recursive`).

RULES: use `gh` (logged in as fschutt). Build: `cargo build -p azul-dll --release --features
build-dll,web-transpiler-static > /tmp/build.log 2>&1`; check `CARGO_EXIT=$?` (do NOT pipe to tail);
`cp -f target/release/libazul.dylib target/release/deps/`. Server: `pkill -f hello-world-v5.bin;
AZ_REMILL_KEEP_SCRATCH=1 AZ_WASM_DEBUG=1 AZ_BACKEND=web://127.0.0.1:8800 ./examples/c/hello-world-v5.bin
> /tmp/server.log 2>&1 &`; poll :8800 (~18 s). Commit each meaningful step, ending the message with
`Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`. Only stage YOUR files
(`core/src/*`, `dll/src/web/*`, `scripts/*`, `third_party/remill/*`) — NOT the parallel agent's
untracked `examples/` files. On cargo lock contention, wait + retry. Disk: `rm -rf target/debug`
(unused) and dead-pid scratch dirs if tight.

GOAL: node_count survives apply_ua_css (`[9]=1`), the cascade produces ≥1 real styled node, and
`hello-world-v5.bin` renders correctly over the web backend.
