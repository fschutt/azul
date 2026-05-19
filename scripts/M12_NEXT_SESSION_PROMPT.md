# M12 — Next-session prompt: close the cascade Box::new init gap

You're continuing work on Azul's web backend. M11 (Sprints 1-8)
landed 10 commits worth of production infrastructure
(`d0199e571..4e5377d80`) — transitive eventloop lift, cascade
call wasm-side, real bbox hit-test, 12-kind TLV decoder, event
wiring, VirtualView infrastructure, flat bench at 94k ops/sec,
docs. See [`STATUS_REPORT_M11_2026_05_19.md`](STATUS_REPORT_M11_2026_05_19.md).

**One issue blocks the next round of features**: the cascade
Box::new init gap. Closing it unblocks real layout, real
VirtualView, and real RefreshDom diff.

## The gap, in 30 seconds

```rust
let styled = StyledDom::create(&mut dom, Css::empty());
let boxed = Box::new(styled);
let ptr = Box::into_raw(boxed) as u32;
// ptr is non-zero ✓
// *(ptr as *const StyledDom).node_data.len() == 0  ✗ (expected ≥ 1)
```

Simple `Box::new(u32)` round-trips (we verified 8888). Complex
aggregates land at the allocated heap but with zero bytes.

## Root cause (per investigation in
`memory/m11_complex_struct_box_new_lift.md`)

AArch64 ABI returns structs > 16 bytes via implicit X8 register
(structure-return pointer). When the lifted `StyledDom::default()`
runs in wasm:

1. Caller sets X8 = destination pointer.
2. Lifted body LOADS X8 into a working register
   (`%53 = load i64, ptr %X8` — visible in the patched IR).
3. **But writes go via X0 / stack-relative addresses, not via
   X8**. So the StyledDom value lands at the wrong location.

A subagent traced this from
`/var/folders/5x/rpb8yr7x6890kpc5886gzv9r0000gn/T/azul-web-transpiler-<pid>/`
contents (kept via `AZ_REMILL_KEEP_SCRATCH=1`).

## Likely fix paths

### Path A — fix the lift's X8 handling in callee bodies (correct, broad)

The lift pipeline at `dll/src/web/transpiler_remill.rs`'s
`emit_helper_ir` correctly handles X8 for ROOT wrappers (via
`Pcs::HiddenPtrReturn` — used by the layout cb). For
TRANSITIVE DEPS (which get the canonical `Callback` signature),
X8 isn't synthesized.

Fix: classify each dep based on whether its native body uses X8
as a destination pointer (analyzable from its first few
instructions — look for `str x0, [x8, #N]` style writes). When
detected, generate the wrapper with `Pcs::HiddenPtrReturn` so
the caller's X8 setup flows through to the State's X8 slot.

The classification could be:
- Scan the first N bytes of the fn for `str/stp` instructions
  with X8 as base register.
- If present + fn is a struct-returning fn, treat as
  HiddenPtrReturn.
- Otherwise treat as canonical Callback (existing behavior).

Risk: false positives (a fn that uses X8 as scratch) would
expect an extra arg the caller doesn't pass.

### Path B — workaround via manual field-by-field construction

Build StyledDom in eventloop.rs by:
1. Allocating each AzVec's storage via `AzStartup_alloc`.
2. Writing each AzVec's `{ ptr, len, cap, destructor }` four
   fields via `core::ptr::write`.
3. Writing each StyledDom field via `core::ptr::write`.

This bypasses `Box::new` of a complex value — the only
operations are basic pointer arithmetic + u32/u64 stores (which
we know lift cleanly).

Bloat: ~100 lines of unsafe pointer arithmetic per StyledDom
field. Tedious but mechanical.

### Path C — hand-write the cascade in LLVM IR

Bypass the Rust→remill path entirely for `StyledDom::create`.
Emit LLVM IR that directly produces a `StyledDom`-shaped blob
from the input `Dom`. The IR has explicit wasm semantics + no
hidden ABI assumptions.

Bloat: ~500 lines of hand-written LLVM IR.

## Recommendation

**Path A** is the right fix long-term. The X8-classifier could be
based on a simple bytes-scan:

```rust
fn callee_uses_x8_sret(fn_bytes: &[u8]) -> bool {
    // Decode first ~16 instructions; check for STR-from-X8
    // or STP-from-X8 patterns.
    for chunk in fn_bytes[..64.min(fn_bytes.len())].chunks_exact(4) {
        let instr = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        // STR Xt, [X8, #imm]: opcode 0xF9000100 with Xn=8 (bits 5-9)
        if (instr & 0xFFC003E0) == 0xF9000100 {
            return true;
        }
        // STP Xt, Xt2, [X8, #imm]: opcode 0xA9000100 family
        if (instr & 0xFFC0001F & 0xFF8003E0) == 0xA9000100 {
            return true;
        }
    }
    false
}
```

Then in the transitive lifter's BFS, when a dep is classified as
struct-returning, the wrapper sig becomes
`Pcs::HiddenPtrReturn` instead of canonical. The caller's lifted
body needs to also pass the X8 arg through — which falls out
automatically if the lift respects AArch64 sret semantics.

**Test plan**:
1. Add the classifier + new wrapper path.
2. Re-run `styled-dom-hydrate.js`. Expect `getStyledDomNodeCount`
   to return ≥ 1 (cascade preserves the body node).
3. Re-run `diff-patches.js` + existing gates.

## What's not affected

The 12 acceptance gates remain GREEN. Sprint 1-8 deliverables
ship as-is. The cascade call lifts + runs, just doesn't
populate StyledDom internals. Workarounds (placeholder block
layout, marker fields) cover the user-visible paths.

## Files to edit

- `dll/src/web/transpiler_remill.rs` — add classifier; update
  `lift_with_transitive_deps_batched`'s dep wrapper sig.
- `dll/src/web/eventloop.rs` — once the cascade actually
  populates, the existing `AzStartup_getStyledDomNodeCount`
  should start returning real numbers.
- `scripts/m9_e2e/styled-dom-hydrate.js` — tighten the gate to
  fail when StyledDom node_count is 0 (currently logs as INFO).

## After the cascade gap closes

The unblocked work cascade (no pun intended):

1. **Real layout solver wiring**: `LayoutWindow::layout_dom_recursive`
   + cascade-derived computed values → real positioned_rects.
2. **Full VirtualView auto-wrap**: layout pass detects
   `NodeType::VirtualView` + invokes the provider; scroll-edge
   triggers re-invocation.
3. **Real RefreshDom diff loop**: re-cascade → re-solve → diff
   old vs new StyledDom via `reconcile_dom_with_changes` → emit
   per-change TLV patches.
4. **`azul-bench-virtual.c`**: 10k rows via VirtualView,
   matching the architectural narrative in M11 hard direction
   #2.

## Branch + commit state

```
Branch: layout-debug-clean
HEAD:   4e5377d80 docs: M11 Sprint 8 final — STATUS_REPORT + superseded banners
```

Build:
```bash
cargo build -p azul-dll --release \
  --features "build-dll web web-transpiler web-transpiler-static" \
  --no-default-features
```

Run all 12 gates per [`STATUS_REPORT_M11_2026_05_19.md`](STATUS_REPORT_M11_2026_05_19.md)'s reproducing section to confirm baseline before starting.
