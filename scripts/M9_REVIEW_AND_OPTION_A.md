# M9 review + Option A done properly (no hacks)

**Date:** 2026-05-18
**Authors:** post-M9 retro + implementation
**Branch state:** `layout-debug-clean` tip `23d7174d5` — synthetic-
address lift shipped.

This document supersedes [M9_WASM_DOM_HANDOFF.md](M9_WASM_DOM_HANDOFF.md)
and [M9_NEW_SESSION_PROMPT.md](M9_NEW_SESSION_PROMPT.md) — the plan
those described was executed (M9 phases 1-6), but the user's post-
review identified the plan as over-architected. The synthetic-
address lift scheme described in §3 LANDED in commit `23d7174d5`;
on_click counter e2e now passes through the full dispatch path
with 128 MiB wasm (was 1 GiB).

## Implementation notes (post-landing)

Three things ended up being slightly more involved than the
plan estimated:

  1. **Stub entries need synth based on their NATIVE LOCATION,
     not their canonical (chase-target) address.** A PLT stub
     in user binary `__stubs` lives at `native_loc`; its
     `canonical_addr` points at the libazul target. If we
     assign synth from `canonical_addr` the stub gets libazul's
     synth, but the lifted caller's `bl` produces user-binary
     synth → mismatch. Fixed by using `by_addr`'s KEY (= the
     entry's native location) instead of `canonical_addr` in
     `assign_synthetic_addresses`.

  2. **The `type_id` mechanism in `AZ_REFLECT_FULL` captures a
     NATIVE address server-side that needs translation.** See
     web.md's "Why `_MyDataModel_RttiTypeId` needed special
     handling" section. `html_render.rs` calls
     `SymbolTable::native_to_synth` on the captured type_id
     before emitting the hydrate JSON.

  3. **The data-segment mirror DOES need to ship**, not just
     re-arrange addresses. Lifted libazul code reads from
     `__cstring`/`__const`/etc.; if the mirror has zeros at
     those offsets, the cb runs to completion but returns
     wrong values. The mirror file size is ~27 MiB for
     hello-world's libazul cover (most of which is the 19 MiB
     `__const`); compresses well on the wire.

What still doesn't work after this lands:

  - **Full `hello-world.c` layout probe** still traps deeper
    in (`wasm-function[103]:0x25b3c`). Same diagnosis as
    before: some libazul function reads from an unmirrored
    section. The fix is incremental — extend the section
    filter in `collect_macho_low32_sections` to include the
    extra sections, or per-symbol classification.
  - **Mini.wasm is 27 MiB.** Reducing this requires either
    splitting the mirror across multiple wasms (one shared
    `/az/data.bin` blob that JS writes once into shared
    memory; smaller mini.wasm) or only mirroring the sections
    actually accessed by lifted code.

## 1. The actual root cause (in one paragraph)

`remill-lift-17` emits one wasm function per native function. For
each instruction it sees, it lifts to LLVM IR that uses
`%program_counter` (the value we passed as `--address`) as the
runtime PC. For ARM64 `adrp x<n>, IMM`, the lifted IR is:

```llvm
%pc = load i64, ptr %PC, align 8
%target_page = and i64 %pc, -4096
store i64 %target_page, ptr %X<n>, align 8
```

When IMM is zero (target in same page as PC, the common case for
nearby `__cstring` literals), `%target_page = page-aligned PC`.
With our current `lift_addr = native_runtime_addr`, the PC is the
post-ASLR runtime address — typically `0x10c0xxxxx` for libazul.
Truncated to 32 bits for wasm32, this lands at `0xc0xxxxx`
(~192 MiB), past the 16 MiB wasm linear memory. **Trap.**

The fix is one parameter: pass a SMALL synthetic `lift_addr`
instead of the runtime address. Then PC is e.g. `0x100abc`,
page-aligned `%X<n>` is `0x100000`, subsequent `ldr` reads from
wasm offset ~`0x100xxx`. Wasm-friendly, no memory inflation,
no data-section mirror tricks at high offsets.

## 2. What M9 actually built and which parts dissolve under the fix

| M9 piece | sound? | post-fix |
|---|---|---|
| `Pcs::HiddenPtrReturn` (M9-1) | ✓ | keep — needed for any aggregate return > 16 B regardless of address scheme |
| `AzStartup_buildLayoutInfo` (M9-2) | ✓ for the minimal case | keep + extend with real `LayoutCallbackInfoRefData` stubs as cbs need them |
| `AzStartup_setLayoutCbTableIdx` / `_setRefAny` (M9-3a) | ✓ | keep — JS-side wiring is correct |
| `__az_call_indirect_layout4` bridge (M9-3a) | ✓ | keep — mechanical mirror of the 3-arg variant |
| `AzStartup_initLayoutCache` (M9-3a) | partial | keep, but extend to do `Dom → StyledDom` + run layout once synthetic addrs work |
| Post-link stack relocator (M9-3a) | ✓ (load-bearing) | keep — independent of address scheme; cross-module wasm SP collisions are real |
| User-binary data section mirror (M9-3b) | scaffolding | KEEP the parsing code, USE different offsets — mirror at synthetic addrs instead of truncated runtime addrs |
| Wasm memory inflation 1 GiB (M9-3b revert) | **wrong** | DELETE — already reverted in `b1470628a`; not needed under synthetic addrs |
| Bump heap relocation to 512 MiB (revert) | **wrong** | DELETE — same |
| `AzStartup_hitTest` stub (M9-4) | stub | replace with real bbox walk once layout cache exists (M9-3a-extended) |
| `AzStartup_buildCounterPatch` (M9-5) | ✓ | keep, generalize beyond u32 |
| loader.js minimization (M9-6) | ✓ | keep — wasm-side dispatch flow is the right shape |
| `AzStartup_dispatchEvent` rewrite (M9-6) | ✓ | keep — uses hydrated refany, emits TLV patches |

Net: of ~1500 lines of M9 code, the ~30 lines that need to change
are the THREE call sites where `lift_addr = native_addr` (plus a
~50 line `SymbolTable` extension to assign synthetic addrs per
image).

## 3. The synthetic-address scheme

### 3.1 Goal

For every loaded image, assign each symbol a `synthetic_addr` such
that:

  - All `synthetic_addr`s fit in `[0 .. 16 MiB)` (well under wasm
    initial memory).
  - PC-relative distances within an image are preserved
    (`synth_B - synth_A == native_B - native_A` for `A, B` in the
    same image).
  - Images don't collide (each gets a unique 4 MiB band, say).

### 3.2 Layout in wasm memory

```
0x000000 .. 0x010000   wasm runtime stack region (mini.wasm slot 0,
                       per-cb / per-layout get distinct slots via
                       the existing `relocate_stack_if_non_mini`)
0x010000 .. 0x100000   user-binary text+data zone (~1 MiB band)
0x100000 .. 0x500000   libazul text+data zone (~4 MiB band)
0x500000 .. 0x600000   libsystem stubs zone (small)
0x600000 .. 0x800000   reserved for future images
0x800000 .. 0x1000000  bump-allocator heap (~8 MiB)
```

Mini.wasm's `initial-memory` stays at 16 MiB. Everything fits.

### 3.3 Symbol-table change

```rust
pub struct SymbolEntry {
    pub canonical_name: String,
    pub canonical_addr: usize,    // runtime addr — kept for dladdr / debugging
    pub synthetic_addr: usize,    // NEW — wasm-friendly address
    pub size: usize,
    pub bytes: Option<&'static [u8]>,
    pub kind: SymKind,
    pub classification: FnClass,
}
```

Build phase: for each image, assign `image_synth_base` (16 KiB
per user binary, 1 MiB per libazul, …). For each symbol:

```rust
synthetic_addr = image_synth_base + (canonical_addr - image_native_text_base)
```

### 3.4 Lift call sites that need to change

Three places currently pass `native_addr` as the lift address:

  - `transpiler_remill.rs::lift_with_transitive_deps_sequential`
    (around line 1083): `let lift_addr = addr as u64;`
  - `transpiler_remill.rs::lift_with_transitive_deps_batched`
    (around line 1253): same.
  - `transpiler_remill.rs::lift_and_link_eventloop`
    (around line 1750): same.

Each becomes:

```rust
let lift_addr = symbol_table::get()
    .and_then(|t| t.lookup(addr))
    .map(|e| e.synthetic_addr as u64)
    .unwrap_or(addr as u64);
```

### 3.5 Cross-function call resolution

For `bl <native_target>` in image `I`:

  - The bl's encoded immediate = `(native_target - bl_native_pc) / 4`
  - Lifted at `synth_pc = synth_base_I + (bl_native_pc - native_base_I)`,
    the lifted call targets `sub_<synth_pc + imm * 4>`
  - `synth_pc + imm * 4 = synth_base_I + (bl_native_pc - native_base_I)
                        + (native_target - bl_native_pc)`
  - `= synth_base_I + (native_target - native_base_I)`
  - `= synthetic_addr_of_native_target` ✓

So intra-image BL targets resolve correctly without rewriting.

Cross-image BLs go via PLT stubs. The stub address is in image
`I` (the caller); the target is in image `J` (the callee, typically
libazul). With per-image synthetic bases, BL to stub resolves via
in-image arithmetic to `synth_of_stub`. The existing
`rewrite_sub_names_to_canonical` chases stubs to their canonical
target — that becomes `synth_of_canonical` after the rewrite.
Already works mechanically.

### 3.6 Data section mirror — what changes

The existing
[`SymbolTable::enumerate_low32_data_for_wasm`](../dll/src/web/symbol_table.rs)
returns `(wasm_offset, bytes)` based on truncated runtime
addresses. After the synthetic scheme: same function returns
`(synthetic_data_offset, bytes)` where:

```rust
synthetic_data_offset = image_synth_base + (data_section_native_addr - image_native_text_base)
```

The mirror now reliably fits below the bump heap (assuming each
image's text+data band is small enough; for hello-world's user
binary that's < 64 KiB, for libazul it's ~22 MiB — too big to
mirror entirely, but the LOADS that hit it are now at predictable
wasm offsets, so a follow-up pass can mirror just the
loaded-from pages).

### 3.7 What this fixes / doesn't fix

✓ Eliminates the OOB trap from `adrp+ldr` to const-string pages
  (the M9-3b core issue).

✓ Eliminates the need for wasm memory > 16 MiB.

✓ Eliminates the data-section-mirror filter heuristic (any image
  whose text+data fits in its allotted band works).

✓ Removes the address-truncation ambiguity (synthetics already fit
  in 32 bits, no truncation surprise).

✗ Does NOT solve the "lifted libazul body needs the right bytes
  in the data segment to read." We still need to MIRROR the
  __const / __data sections, but now at predictable synthetic
  offsets so the mirror tooling is straightforward.

✗ Does NOT eliminate libsystem (libc-style) calls in user
  callbacks (`snprintf`, `strlen`). These still get `Leaf` stubs
  with X0=0 returns. Replacing them with real wasm-side impls
  is independent work.

## 4. Open question: why not "don't share wasm memory + JS serialize"?

The user asked whether giving each wasm module its OWN memory and
serializing data through JS would work. Quick analysis:

  - **Pro**: each module is self-contained; no stack / heap /
    data overlap concerns. The stack relocator becomes
    unnecessary.
  - **Con**: each module still bakes the SAME high addresses
    into its `adrp` lifts. Each module would need its OWN memory
    sized to absorb those addresses — moving the problem rather
    than fixing it. (Unless we ALSO use the synthetic-address
    scheme; combined approaches.)
  - **Con**: every cross-module call (mini → cb, cb → mini for
    __az_call_indirect) now needs JS-side bridging — slower and
    drags JS back into the dispatch loop the user wants empty.

So separate-memory ISN'T the fix on its own; combined with
synthetic addrs it adds complexity without independent benefit.
**Synthetic addrs + shared memory is the right Option A.**

## 5. Open question: "reserve space for exactly one callback"

The user also suggested static slot reservation. This is a
HEAP MANAGEMENT optimization (avoid bump-allocate per call), not
an address-space fix. Useful as a future memory-usage refinement
but doesn't address the lift's high-address bake-in.

## 6. Suggested next-session work order

1. Add `SymbolEntry::synthetic_addr` + per-image assignment in
   `symbol_table.rs` (~50 LOC).
2. Switch the three `lift_addr = native_addr` call sites to
   `synthetic_addr` (~10 LOC; verify nothing else relies on the
   identity).
3. Update `rewrite_sub_names_to_canonical` to use synthetic addrs
   for the rewrite (~20 LOC tweak).
4. Update `inject_user_binary_data_segments` to use synthetic
   offsets (~20 LOC tweak).
5. Run regression: M8.9 on_click counter e2e in both modes,
   minimal layout probe.
6. Test full `hello-world.c` layout probe — first time the cb
   should run without trapping (it may still produce wrong data
   if some libazul __const bytes aren't mirrored, but the trap
   itself goes away).
7. If trap goes away, layer in the M9-3a `Dom → StyledDom`
   conversion + `LayoutWindow` embed for real hit-test.

Total: probably one session of work for steps 1-5 (low-risk
mechanical changes), one for step 7 (high-risk: integrating
azul-layout's solver3 into the wasm-resident path).

## 7. Hacks still on the books after the synthetic-addr fix

From the original `HACKS_REVIEW_2026_05_16.md`, items that REMAIN
real after the fix:

  - #3 button styling on first render — independent of lift
  - #9 wrapper sig per cb kind — independent of lift (M9-1
    addressed the layout cb specifically)
  - #11 hand-rolled `AzStartup_hydrate` — depends on lifting
    user's `_fromJson`, which depends on the user binary's const
    strings being accessible (which the synthetic fix addresses)
  - #13 4 KiB stack alloca per wrapper — independent
  - #18 hardcoded hydrate schema — depends on #11

Items that DISSOLVE under the fix (in addition to the ones
SymbolTable already retired):

  - "Wasm memory inflation" — never makes sense
  - "Data section mirror filter heuristic" — no filter needed
  - The 1 GiB / 3 GiB experiments from late M9 — dead ends

## 8. Documents this review supersedes

  - `M9_WASM_DOM_HANDOFF.md` (now marked SUPERSEDED)
  - `M9_NEW_SESSION_PROMPT.md` (now marked SUPERSEDED)
  - The "memory expansion to 1 GiB" commits (`ea4c82584`,
    `fcc2ee11d`) and their revert (`b1470628a`) — keep in the
    log for history; no functional dependency.

## 9. Documents this review affirms

  - `HACKS_REVIEW_2026_05_16.md` — still accurate. The
    "SymbolTable is the one bug" framing was right; this review
    just identifies a SECOND bug at the same layer (lift_addr
    convention) that the SymbolTable refactor didn't surface
    because hello-world's on_click didn't exercise `adrp`.
  - `STATUS_REPORT_M9_2026_05_18.md` — accurate snapshot of M9
    end-state; this review explains the *why* behind the gaps it
    identified.
  - `doc/guide/en/internals/web.md` — updated post-M9 to match
    reality; should be re-checked once the synthetic-addr fix
    lands.
