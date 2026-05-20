# HANDOFF — M12.7: lift the full layout solver (azul web backend)

> **You are continuing the azul web backend.** The CSS cascade now lifts +
> runs end-to-end in WebAssembly. Your job: get **layout** to run wasm-side
> too — lift `LayoutWindow::layout_dom_recursive` (taffy flexbox/grid/block)
> so the served page is positioned by the real solver, not a stub.
>
> **READ THIS WHOLE FILE FIRST.** It lists the bugs we already hit + the
> debugging method that works. Do not re-derive them.

---

## 0. TL;DR of where things stand (2026-05-20)

The web backend lifts AArch64 machine code from `libazul.dylib` → wasm32 via a
remill-based transpiler, and runs the real azul pipeline in the browser
(cascade now; layout next). As of commit `0bf6241a9`:

- **Cascade works E2E.** `StyledDom::create(dom_ref, Css::empty())` lifts +
  runs in wasm; `getStyledDomNodeCount == getDomNodeCount == 1` for
  hello-world-v5 (a single `body` node). No traps, no OOB.
- **Layout is still a STUB.** `AzStartup_solveLayout` (dll/src/web/eventloop.rs)
  is a block-flow placeholder: "each node gets (0, y, viewport_w, H)". It does
  NOT run real CSS layout. **This is your target.**

The real solver is **`LayoutWindow::layout_dom_recursive`**
(`layout/src/window.rs:854`). It pulls in **taffy 0.10** (flexbox / grid /
block_layout / calc — see `layout/Cargo.toml:52`), so this is a MAJOR lift on
the scale of the cascade. Budget multiple sessions.

---

## 1. The two root causes we already fixed (DO NOT re-debug these)

### RC#1 — SP / callee-saved register leak  (fix: `AZ_FIX_SP`, now DEFAULT)
Lifted `CssProperty::clone` (and any `-> !` / early-exit path) **drops its
epilogue `add sp, #N`** and leaks the guest SP into the caller's frame. Over
many calls this drifts `create_from`'s SP-relative cache base toward NULL →
the infamous "node_count corruption / 768 MiB OOB".
**Fix** (`dll/src/web/transpiler_remill.rs::enforce_sp_preservation`): wrap
every lifted `call sub_<hex>(state,…)` with a load-before / store-after of the
12 callee-saved State slots (X19-X28, X29/FP, SP = byte offsets
`[848,864,880,896,912,928,944,960,976,992,1008,1040]`). DEFAULT-on; disable
with `AZ_NO_FIX_SP=1`. **Layout code will hit the same class of bug** — the
wrap already covers it, but watch for it.

### RC#2 — libc `memcpy` lifted as a no-op  (fix: `FnClass::LibcMemcpy`)
`Box::new(big_struct)` / slice copies lower to an out-of-line **`bl _memcpy`**
(libsystem stub, PLT-chased to an out-of-image address). `classify_for_name`
returned the default `Leaf` for it, and **the Leaf stub RETURNS without
copying** → the destination keeps its zero-init bump bytes (e.g.
`Box::new(styled)` left `node_data.len == 0`).
**Fix** (`dll/src/web/symbol_table.rs` + `transpiler_remill.rs`): new
`FnClass::LibcMemcpy`; `classify_for_name` maps memcpy/memmove (all spellings:
`_memcpy`, `_platform_memmove`, `___memcpy_chk`, …); the M10-A1 out-of-image
pass exempts it; `emit_helper_ir` emits a real `@llvm.memmove` body. DEFAULT.
**Layout's taffy code does lots of struct/slice copying — if a copy silently
produces zeros, suspect another libc primitive (memset? bcopy?) being a no-op
Leaf. The fix pattern is the same: add a real body in the `branch_stubs` match.**

### RED HERRINGS — proven false, do NOT chase again
- ❌ **"NEON-Q / `ldp q`/`stp q` mis-lift"** — DISPROVEN. A direct
  `remill-lift-17 --bytes a00640ad000400ad` (ldp q0,q1,[x21]; stp q0,q1,[x0])
  showed they lift CORRECTLY (decomposed into 4× `__remill_read/write_memory_64`).
  remill's NEON Q-pair semantics are fine.
- ❌ **"inline u128 / `1u128 << d` corrupts the cache"** — false; node_count is
  1 throughout once AZ_FIX_SP is on.
- ❌ **"remill register-VALUE mis-tracking writes 0 to node_count@self+0"** — the
  whole "self(X0)-relative store / node_count phantom" trail was chasing reads
  off a transiently-NULL cache base caused by RC#1, plus the RC#2 memcpy no-op.
- ❌ **"`grow_one` / Vec growth never lifts"** — fixed long ago (commit
  `999f2abdc`: `alloc::raw_vec` is `Recursable`, not `Leaf`). Keep it.

See memory `m12_cascade_neon_blocker.md` for the full (long) trail.

---

## 2. The lift architecture (how to add layout)

- **Pipeline** (subprocess, default): `remill-lift-17 --bytes <hex>` → `.lifted.ll`
  → patched → `+ helper.ll` (the `branch_stubs`) → linked → `opt -O2` →
  `.opt.ll` → `llc -mtriple=wasm32` → `.o` → `wasm-ld`. Native in-process path
  exists behind `AZ_NATIVE_REMILL=1` (not needed; the helper-IR fix runs in both).
- **Transitive lifter** walks `bl` targets from each `AzStartup_*` entry,
  classifying each via `symbol_table.rs::classify_for_name` → `FnClass`:
  - `Recursable` — our crates (azul_*, taffy will be here) → lift + recurse.
  - `Leaf` — system/libc/runtime → typed extern stub (NO body). **The trap:** a
    Leaf that should actually DO something (memcpy) silently no-ops.
  - `BumpAlloc`/`BumpRealloc`/`LibcMemcpy`/… — synthetic helper-IR body emitted
    in `transpiler_remill.rs` `branch_stubs` match. **Add new ones here when you
    find another mis-classified primitive.**
- **State struct** byte offsets (remill AArch64): X0=544, X1=560, X2=576,
  X3=592 (stride 16); X19=848 … X28=992, X29=1008, SP=1040, X8(sret)=... (grep
  `x0_off` in transpiler_remill.rs — it's 544).

---

## 3. The debugging method that WORKS (use these, in this order)

1. **otool disasm of the original** — `otool -arch arm64 -tV
   target/release/deps/libazul.dylib`, `awk` to the function. Shows whether a
   move is `bl _memcpy`, inline `ldp/stp q`, etc. THIS is how RC#2 was found.
2. **Direct single-instruction lift** — extract bytes
   (`python3 -c "open(dylib,'rb').seek(off); ..."`), then
   `remill-lift-17 --arch aarch64 --os macos --address 0xADDR --bytes <hex>
   --ir_out /tmp/x.ll`. Proves whether remill decodes/lifts an instruction
   correctly IN ISOLATION (defeats "is it the lift or the context" ambiguity).
   `remill-lift-17` = `third_party/remill-install/build/remill/bin/lift/`.
3. **server.log `resolved=… class=…` lines** — the transitive lifter logs every
   dep: `dep: sub_XXXX → resolved=NAME@0x… class=Leaf`. Grep for a suspicious
   symbol to see how it was classified. THIS confirmed memcpy=Leaf.
4. **Lift artifacts** — run the server with `AZ_REMILL_KEEP_SCRATCH=1`; the
   scratch dir (`$TMPDIR/azul-web-transpiler-<pid>/`) keeps every
   `<fn>.lifted.ll` / `.opt.ll` / `.o`. Grep the `.opt.ll` to see the FINAL
   wasm-bound IR (post-opt SSA addresses are fixed → faithful).
5. **AZ_LOG_STORES post-opt store tracer** (env-gated, kept on purpose) —
   `inject_store_logging` in transpiler_remill.rs instruments every
   store/memset/memcpy in the post-opt IR with an address+value log to a wasm
   ring buffer. `AZ_LOG_STORES=ALL` or a comma-list of fn-name substrings.
   Heisenbug-proof (instruments fixed post-opt addresses, not running code).
6. **`AzStartup_peekU32(addr)`** — read any wasm linear address from JS.
7. **AVOID:** runtime Rust `eprintln`/probes inside lifted code — they perturb
   register allocation (Heisenbug). Use the static/IR-level tools above.

---

## 4. The layout lift — concrete plan

**Goal:** replace the `AzStartup_solveLayout` stub with a call into the real
solver, lifting its transitive deps.

1. **Find the desktop call site** of layout to copy its exact shape: grep for
   `do_the_layout` / `layout_dom_recursive` / `LayoutWindow::new` in
   `layout/src/window.rs` + `core/src` + `dll/src/desktop`. It consumes a
   `StyledDom` (you now build one → `current_dom_styled_ptr`) + a viewport
   `LogicalSize` and produces positioned rectangles (`PositionedRectangle` /
   `LayoutResult`).
2. **Add a new `AzStartup_solveLayoutReal`** (or rewrite `solveLayout`) in
   `dll/src/web/eventloop.rs` that: takes the StyledDom ptr + viewport w/h,
   calls the real solver, writes the resulting per-node rects into the existing
   `positioned_rects_ptr` buffer (same format hit-test already reads — see
   `AzStartup_hitTest`). Register it in `mod.rs`'s export list + add its
   `CallbackSignature` in `transpiler_remill.rs` (mirror `solveLayout`).
3. **Lift it.** Expect the transitive walk to pull in a LOT of taffy +
   azul-layout code. Watch the server.log for:
   - new `class=Leaf` symbols that should have bodies (libc primitives →
     add to `LibcMemcpy`-style helper bodies, or a new `FnClass`),
   - `__remill_missing_block` / decode failures (→ direct-lift the bytes to see
     which instruction; may need a remill-fork semantics addition),
   - traps / OOB (→ usually RC#1-class SP leak in a new callee, or an unhandled
     intrinsic).
4. **Iterate** with the §3 tools. Each new gap is the same shape as RC#2: find
   the symbol, see why its lift is wrong, add a real body or fix the classifier.

**taffy specifics to watch:** taffy uses `f32` math, `Vec`/`SmallVec`,
`grid`/`flexbox` with lots of small-struct returns (sret) and slice copies.
Float intrinsics (`fmod`, `roundf`, etc.) may come through as libc Leafs →
no-op → wrong layout. Bulk copies → memcpy (already fixed) but also possibly
`memset`/`bzero`. Check each with the direct-lift test.

---

## 5. Verify / build / run

```bash
# Build (the server binary loads target/release/libazul.dylib dynamically):
cargo build -p azul-dll --release --features build-dll,web-transpiler-static

# Run the server (AZ_FIX_SP + LibcMemcpy are DEFAULT now; no env needed):
pkill -9 -f hello-world-v5.bin; sleep 2
DYLD_LIBRARY_PATH=target/release AZ_BACKEND=web://127.0.0.1:8800 \
  nohup ./examples/c/hello-world-v5.bin > /tmp/server.log 2>&1 &
# (add AZ_REMILL_KEEP_SCRATCH=1 to keep .opt.ll artifacts for inspection)

# Gates (must stay green):
node scripts/m9_e2e/full-cycle.js     # bootstrap→layout→click→hit-test→cb→patch
node scripts/m9_e2e/hit-test.js       # positioned-rect walk
node scripts/m9_e2e/baseline-probe.js # getStyledDomNodeCount / getDomNodeCount / hydrateRc
# NEW gate to add: a layout-correctness probe that asserts the solved rects
# match expected CSS layout (not the stub's stacked rows).
```

`hello-world-v5` is a single-node body — to actually exercise layout you'll
likely want a richer example (nested divs, flex). The parallel agent has been
adding examples under `examples/`; coordinate before adding more.

---

## 6. Rules / constraints

- **Stage only YOUR files**: `core/src/*`, `dll/src/web/*`, `scripts/*`,
  `third_party/remill/*`. **Do NOT stage the parallel agent's untracked
  `examples/` files** (hello-world-v1/2/3.c, go/*, kotlin/*).
- **remill fork**: branch `m12-q-reg-x8-sret` in `third_party/remill`. If you
  add a semantics/decoder, commit to the fork branch and push to the FORK only —
  **no PR/MR**. Rebuild it with `scripts/build_remill.sh` (slow; LLVM).
- `gh` is logged in as `fschutt`; remote `origin` = github.com/fschutt/azul.
- End commit messages with:
  `Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>`
- On cargo lock contention, wait + retry.
- Memory: `~/.claude/.../memory/m12_cascade_neon_blocker.md` has the full
  resolution + this plan. Update it as you go.

---

## 7. Commit map of the cascade fix (for context / bisecting)

```
999f2abdc  grow_one Leaf→Recursable (KEEPER, earlier)
3a779d9bc  THE FIX — lift libc memcpy as a real @llvm.memmove body (FnClass::LibcMemcpy)
b5fd492fa  make AZ_FIX_SP (SP/callee-saved preservation) the default
20aae34fe  strip runtime debug scaffolding from hydrateStyledDom
5bd8838aa  remove unused make_test_* reproducer defs
5f1538ac6  M12.7 start — cascade the layout-cb's real Dom (dom_ref)
0bf6241a9  cleanup — remove M12 cascade debug instrumentation (AZ_DBG_NC etc.)
```

Good luck. The method in §3 is what cracked the cascade — apply it to taffy.
