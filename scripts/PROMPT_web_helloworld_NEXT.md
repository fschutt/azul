# Next-agent run prompt — get hello_world.c working on the azul web (lifted) backend

Copy the block below as the task prompt for the next agent run.

---

Goal: make `examples/c/hello-world.c` (a styled button + counter) render and react on the azul **web
(lifted) backend** — native ARM64 `libazul.dylib` remill-lifted to `azul-mini.wasm`, layout/shaping in wasm.

Branch `mobile-ios-android`, repo `/Users/fschutt/Development/azul-mobile`, remill fork
`/Users/fschutt/Development/azul/third_party/remill`. Do NOT commit unless asked.

START by reading `scripts/HANDOFF_web_helloworld_NEXT_2026_06_06.md` in full — it has the current state,
the systemic root cause, the build/run recipe, the gotchas, and the keeper-vs-revert list. Background
detail: `scripts/HANDOFF_web_vec_return_len_mislift_2026_06_06.md` + memory
`web_vec_len_mislift_systemic_2026_06_06.md`.

Current state (do NOT re-derive): the original "Vec-return `len` mis-lift" / 1.6 GB OOB is FIXED
(out-param); text SHAPES + MEASURES for `web-text-min.c`. "InvalidTree" was a harness PHANTOM (never-written
0x40120) — ignore it. The remaining blocker is a SYSTEMIC remill lift-fidelity failure in OPTIMIZED Rust
code: SROA'd `Vec::len()` reads 0 (1 via volatile), sret/NRVO aggregate returns mis-lift, and `for`-loops
over ranges/iterators iterate 0 times. It hits `std::collect`/`std` iterators, so per-site source
workarounds CANNOT fix it (4 forms tried, all failed — g136–g139). The single fix that unblocks text
positioning AND hello_world.c is the transpiler/remill optimized-code fix.

Do these in order:

1. **CHEAP FIRST — lower-opt experiment.** Add `[profile.release.package.azul-layout] opt-level = 1` (try
   also `azul-core`, `azul-css`) to the workspace Cargo.toml, rebuild + relift `web-text-min.c`, run
   `AZ_LENIENT=1 node scripts/m9_e2e/layout-flexbox.js`, and check `[g132 lays-out] overflow_size.height`.
   The mis-lifts are optimizer-induced (SROA/iterator inlining); lower opt may make the lifted loop EXECUTE
   correctly. If the `dom_children` loop now enters and text positions (h>0) → that is the pragmatic
   unblock; proceed to hello-world.c. (Use opt-level 1 or "s", NOT 0 — 0 overflows the wasm local limit.)

2. If (1) doesn't unblock it, do the **remill execution-fidelity investigation** (deep): lift the full
   `collect_and_measure_inline_content_impl` in isolation (`nm libazul.dylib | grep
   collect_and_measure_inline_content_impl` → native `0x26f488`/`0xabf5fc`;
   `remill-lift-17 -bytes <hex> -address <addr> -ir_out /tmp/x.ll`) and EXECUTE/trace the lifted loop vs
   native to find why the iterator/len mis-executes (a spill/reload/PHI the lift mis-models). Fix it in the
   remill fork or the transpiler post-pass. This unblocks ALL Vec/iterator/sret sites at once → then revert
   the out-param + g137/g139 workarounds and the diagnostic markers (§4 of the handoff).

3. Once `web-text-min.c` text POSITIONS, move to `hello-world.c` and work its known extra blockers
   (handoff §6): button cascade styling (CssProperty jump-table), auto-height (content vs Percent(100)),
   `__snprintf_chk` counter text (Leaf stub), and the click/dispatchEvent path. Many are the same systemic
   mis-lift and may fall out together.

Heed the gotchas in handoff §5: `AZ_NO_LIFT_CACHE=1` (AZ_LIFT_CACHE hangs), poll the port not the pid, kill
orphan remill workers between runs, relifts are ~15-30 min (don't burn context polling — use a tracked
background waiter + the harness). Keep diagnostic markers wasm-only (they SEGV the native server otherwise).

Keep working toward hello_world.c rendering; document findings + the next concrete step in
`scripts/HANDOFF_web_helloworld_NEXT_2026_06_06.md` as you go.
