# M8.7c Hello-World Hacks Review — 2026-05-16

**Context**: hello-world counter now mutates through a wasm-compiled
on_click cb (see `7530483aa`). The path from "lift works" to
"actually production-ready" still has many hacks, hardcoded values,
and platform assumptions. This document is the honest catalog.

**Backup**: `backup/m8.7c-victory-2026-05-16` branch + `m8.7c-victory`
annotated tag both point at `7530483aa`.

---

## Critical — broken or unimplemented things the user actually noticed

### 1. DOM patching doesn't visibly update in a real browser
`loader_js.rs:azInvokeCbDirect` reads the post-cb counter from
`wasm-mem[azModelPtr]` and calls `document.getElementById('az_1').textContent = ...`.

**Hacks here:**
- `az_1` is hardcoded. Works for hello-world because the only text
  node is the counter; would silently no-op or trample the wrong
  node on any other app.
- No diff against the previous DOM — we don't know WHICH text node
  changed without re-running layout in wasm and diffing against
  the cached StyledDom.
- The "counter is at offset 0 of MyDataModel" assumption is also
  hardcoded; for any other type we'd have no idea what byte to read.
- Even with the right node ID, if the change requires re-layout
  (text expanding past its box, new children appearing), we'd need
  to re-render the affected subtree's HTML — not just patch a
  textContent.

**Real fix (M8.5d/M8.7d):** call the lifted layout cb after the cb
runs, diff the resulting StyledDom against the cached one, emit TLV
patches (the `azApplyPatches` decoder already handles `SetText` —
just need a real producer instead of the JS-side guess).

### 2. Mousemove / hit-test / dispatch chain doesn't work
Only `click` is wired in `azWireListeners`. The previous
`azDispatch` chain that called `mini.AzStartup_dispatchEvent` with
event coords + buffer is gone — replaced with `azInvokeCbDirect`
that bypasses mini entirely.

**Consequences:**
- No mousemove/hover effects.
- No keyboard input.
- No focus tracking.
- No resize/scroll handling.
- Hit-test runs JS-side via `azNodeIdxFromEvent` (regex on element
  IDs) instead of wasm-side against laid-out coordinates.

**Real fix:** restore `AzStartup_dispatchEvent` as the entry point.
Have it use `azRefAnyPtr` (passed via `AzStartup_init` or a new
`AzStartup_setAppData` helper) instead of `FAKE_REFANY_LO = 0x101`.
Then re-wire all event listeners through it.

### 3. Button doesn't look like desktop on first render
`html_render.rs` walks `StyledDom` and emits CSS via the cascade,
BUT:
- It only handles a subset of computed properties — anything not
  emitted as a `#az_N { ... }` rule never makes it to the browser.
- Bundled themes (e.g. azul's native button styling that lives in
  the layout crate's default CSS) don't get inlined into the
  emitted stylesheet.
- The `__azul-native-button __azul-btn-primary` class names in the
  HTML are leftover from the desktop renderer that uses CSS the
  browser doesn't have.

**Real fix:** ship the desktop's bundled CSS as a `<style>` block
in the head (or as a stylesheet link), and make sure
`html_render.rs` doesn't strip the class names the bundled CSS
expects.

---

## Lift-pipeline hacks (`dll/src/web/transpiler_remill.rs`)

### 4. `LIFT_READ_WINDOW = 4096` is fixed
**Where:** `mod.rs::LIFT_READ_WINDOW`.

**Hack:** every lift reads exactly 4 KiB starting at the symbol
address. Works because remill stops at the first `ret`, so
over-reading into the next function is harmless. But:
- A function bigger than 4 KiB would be truncated mid-body.
- We're reading 4 KiB even for 12-byte stub thunks.
- On `__TEXT.__stubs` PLT trampolines we're reading way past the
  stub into other stubs and treating the result as semantically
  meaningful.

**Real fix:** parse the host binary's symbol table (Mach-O LC_SYMTAB
on macOS, ELF `.symtab` on Linux) at server startup and look up
the exact size per symbol via the next symbol's address. This is
~150 lines of platform-specific code; we should have one for each
OS.

### 5. PLT-stub resolver is macOS arm64 only
**Where:** `mod.rs::resolve_macos_arm64_stub`.

**Hack:** parses the exact `adrp x16, GOT_PAGE ; ldr x16, [x16, GOT_OFF] ; br x16`
instruction pattern emitted by ld64 on Apple Silicon. Returns
`None` on every other platform/architecture, falling back to the
`cb_<hex>` placeholder symbol name.

**Real fix:** per platform/arch matrix:

| Platform | Arch | PLT pattern | Status |
|---|---|---|---|
| macOS | arm64 | `adrp+ldr+br x16` | implemented |
| macOS | x86_64 | `jmp qword ptr [rip + GOT_OFF]` | not implemented |
| Linux | arm64 | `.plt` entries (same adrp+ldr+br) | likely works as-is, untested |
| Linux | x86_64 | `.plt` entries (push + jmp) | not implemented |
| Windows | any | IAT trampolines | not implemented |

Better long-term: parse the host binary's import table directly
(Mach-O `__DATA.__got` / ELF `.got.plt` / PE IAT) so we don't have
to disassemble stubs at all.

### 6. Tail-call wrapper byte rewrite is arm64 only
**Where:** `transpiler_remill.rs::rewrite_tailcall_wrapper`.

**Hack:** detects single-instruction `B imm26` arm64 functions and
flips bit 31 to convert to `BL` + appends a `RET`. Falls through
to original bytes on every other arch.

This is fixing a problem with how *we* invoke remill (it bails on a
bare tail-call branch), not a real semantic issue.

**Real fix:** either (a) configure remill to follow tail calls as
calls + synthetic returns, or (b) per-arch rewriters (`jmp imm32`
on x86_64 → `call imm32 + ret`).

### 7. Recursion filter has a hand-curated allowlist
**Where:** `transpiler_remill.rs::is_recursable_dep`.

**Hack:** matches mangled-name prefixes against a fixed list of
"runtime crates" (core, std, alloc, compiler_builtins,
panic_abort, panic_unwind, rustc_demangle, backtrace, addr2line,
gimli, object, miniz_oxide) to know what NOT to recurse into.

Adding a new dep that pulls in a new runtime crate could either:
- explode the lift depth into the new crate's call graph (if it
  isn't on the list); or
- silently noop important semantics (if the list grows too
  aggressively).

**Real fix:** the user's "pre-compile-all api.json" architecture
inverts this — the api.json gives us the authoritative list of
"things we want lifted". Anything outside the api.json set + the
user's own callbacks is a leaf.

### 8. Helper-IR no longer emits noop bodies for Noop kind
**Where:** `transpiler_remill.rs::emit_helper_ir`.

**Side effect:** any extern not covered by another `.o`'s real
body becomes an `env.sub_<hex>` import that JS's Proxy fallback
satisfies with shape-guessed noops (i32→0, i64→0n, void→undefined).
The Proxy guesses via name patterns (`*_64` → i64 etc.).

**Hacks here:**
- Shape guessing by name. Anything that returns an i32 but happens
  to have `_64` in its name (unlikely, but possible) gets a BigInt
  back and the call traps.
- Noop semantics for symbols that have real behavior — we lose
  e.g. `_ZN3std4sync...rwlock...` if the program ever takes that
  path; the cb silently produces wrong state instead of asserting.

**Real fix:** scan unresolved imports at wasm-build time and either
(a) emit a typed JS stub per import, or (b) compile a tiny "all
unresolved are panics" body that traps loudly when called.

### 9. Per-cb wrapper signature is hardcoded to `(i64, i64, i32) → i32`
**Where:** `transpiler_remill.rs::signature_for_callback_kind` and
all `__az_dep_<addr>` wrappers.

**Hack:** every lifted dep is wrapped under the canonical Callback
shape regardless of the dep's actual signature. Works because:
- Internal calls (from one lifted body to another via `sub_<addr>`)
  use remill's `(state, pc, memory)` convention and bypass the
  wrapper entirely.
- JS-callable wrappers (`__az_dep_<addr>`) get called with shape-
  guessed args during debugging; in production no one calls them.

But:
- `Callback`'s args claim `AzRefAny` is a 16B aggregate split
  across X0+X1, when it's actually a 24B aggregate passed by
  hidden pointer in X0 on arm64 PCS.
- Functions returning >16B (StyledDom, AzResultRefAnyString, ...)
  use X8 as a hidden return-buffer pointer; the canonical sig
  doesn't model X8 at all.
- The few sig variants we do define (CheckBoxOnToggle, the
  AzStartup_* fns) are hand-rolled per fn — not derived from
  api.json. Adding a new typedef requires a manual sig entry.

**Real fix:** parse the api.json's argument types per fn and
synthesize a per-sig wrapper, including hidden-X8-return for
aggregates >16B. Generic per-platform PCS table.

### 10. Eventloop `lift_addr = native_addr` works but isn't load-bearing
**Where:** `transpiler_remill.rs::lift_and_link_eventloop`.

Notably, this works *because* LLVM inlined every `AzStartup_alloc`
call inside other `AzStartup_*` fns, so the cross-fn calls were
gone before lift. If a future eventloop fn wasn't inlined, we'd
need real cross-fn linking — currently relies on inlining luck.

**Real fix:** explicit thunk emission for cross-eventloop calls
(same mechanism as the PLT-stub thunks for libazul calls).

---

## Hydration hacks (`dll/src/web/eventloop.rs::AzStartup_hydrate`)

### 11. `AzStartup_hydrate` is a hand-rolled RefAny builder
**Hack:** does NOT call the user's registered `_fromJson` deserializer
(the AZ_REFLECT_JSON pathway). Instead allocates 128B for
RefCountInner + 32B for RefAny and writes the fields directly.

**Hardcoded values:**
- `data_align = 8` (assumes anything user passes is 8-byte aligned).
- 128B upper bound for RefCountInner (real size is ~112B; padding
  is wasteful but harmless).
- 32B upper bound for RefAny (real size is 24B).
- `run_destructor = false` (the cleanup never runs — small leak
  per server lifetime, but the bigger issue is the user's
  destructor never fires).
- `instance_id = 0` (no unique clone-id; clone tracking is broken).
- `type_name = ""` (no debug name).
- `serialize_fn = 0`, `deserialize_fn = 0` (cb can't re-serialize
  the data once mutated).
- `num_copies = 1`, `num_refs = 0`, `num_mutable_refs = 0` —
  hardcoded singleton; multi-borrow tracking starts wrong if the
  cb wants to share/clone the RefAny.

**Real fix:** lift the user's `_fromJson` fn-ptr (added via
`AzStartup_registerStateDeserializer`) and call it via
`__az_call_indirect` from inside AzStartup_init. That's what
M8.7 hydration was originally spec'd to do; we cut corners
because `_fromJson` has a non-canonical signature
(`AzJson → AzResultRefAnyString`) that the wrapper synthesis
doesn't handle yet.

### 12. JS-side hydrate hardcodes `data_size = 4`
**Where:** `loader_js.rs::azHydrate`.

```js
azModelPtr = azMini.AzStartup_alloc(4);
new DataView(azMemory.buffer).setUint32(azModelPtr, counter >>> 0, true);
azRefAnyPtr = azMini.AzStartup_hydrate(typeIdLo, typeIdHi, azModelPtr, 4);
```

**Hack:** assumes the user's data is exactly a `u32`. Hello-world's
MyDataModel is `{ counter: u32 }`. Anything else (a struct with
strings, nested aggregates, references) silently misencodes.

The az-hydrate payload's `json` field is only read for `typeof ===
'number'`; bool/string/array/object don't work.

**Real fix:** ship the user-data size in the hydrate payload
(server-side knows it from `app_data.get_type_size()`). For
non-primitive types, use the lifted `_fromJson` path (item 11)
rather than direct memcpy.

### 13. The 4 KiB stack inside each wrapper is hardcoded
**Where:** `transpiler_remill.rs::emit_helper_ir` (the wrapper
template emits `alloca [4096 x i8]`).

**Hack:** every wrapper allocates 4 KiB of stack for the lifted
body's SP-relative spills. Functions with deeper stack usage
(recursion, large local arrays) would overflow silently. Plus
every cb invocation pays 4 KiB regardless of need.

**Real fix:** scan the lifted IR for max SP displacement, size the
alloca accordingly. Or use a single shared stack region that all
wasms grow into.

---

## Bootstrap-flow hacks (`dll/src/web/loader_js.rs`)

### 14. `azInvokeCbDirect` bypasses `AzStartup_dispatchEvent`
Already covered in item 2. Worth restating: the whole reason we
have a mini wasm is to host the event-loop logic; bypassing it
means none of mini's plumbing (event decode, hit-test, cb-cache,
patch emission) runs.

### 15. JS Proxy noops for unresolved env imports
**Where:** `loader_js.rs::azMakeMiniImports` and `azCallbackImports`.

**Hack:** any unresolved symbol becomes a Proxy-served noop. Name-
pattern shape guessing (covered in item 8).

**Production concern:** silently producing wrong return values for
real-but-unlifted fns is worse than trapping — you debug a wrong-
counter bug for a long time before realizing some fn silently
returned 0 when it shouldn't have.

### 16. `azNodeIdxFromEvent` walks `id="az_N"` regex
**Where:** `loader_js.rs::azNodeIdxFromEvent`.

**Hack:** hit-test by walking up the DOM tree and matching element
IDs against `/^az_(\d+)$/`. Works because every cb-bearing node
gets emitted with that ID. But:
- If the user has custom IDs, we don't see them.
- Pseudo-event-targets (e.g. `mouseenter` on a child of the cb-bound
  parent) aren't disambiguated.
- No coordinate-based hit-test — the user can't bind a cb to
  "wherever the cursor is in this canvas".

**Real fix:** wasm-side hit-test using the hydrated layout cache
(M8.5c).

### 17. Per-cb table slot key uses `node_idx` directly
**Where:** `loader_js.rs::azFnAddrToTableIdx`.

**Hack:** `azFnAddrToTableIdx.set(nodeIdx, nodeIdx)` — the
`__az_resolve_callback` JS bridge maps "fn-addr" (which the mini
calls with `node_idx`) to a table slot of the same index. Per
the comment "M8.5c+ will swap to real fn-addrs harvested from a
hydrated StyledDom".

So the entire dispatch architecture currently fakes the addr→cb
indirection.

---

## Server-side hacks (`dll/src/web/html_render.rs`)

### 18. Embedded `az-hydrate` schema is hello-world-specific
```html
<script id="az-hydrate" type="application/json">
{"type_id":"...","json":5}
</script>
```

The `json` field is whatever the user's `_toJson` returns. For
non-number types, the JS side wouldn't know how to decode and
hydrate.

**Real fix:** embed the user-data BYTES (base64'd) + the
`_fromJson` fn-addr, and let the wasm side run the lifted
deserializer.

### 19. Stylesheet emission is best-effort
Already covered in item 3.

---

## Things that ARE principled and worth keeping

Counter-balancing the list: these aren't hacks, just normal
implementation choices that should survive a rewrite:

- **api.json walk** — drives off the embedded brotli-compressed
  api.json, no hardcoded function lists.
- **bump allocator in helper IR** — minimal, well-isolated, and
  matches the wasm linear-memory model.
- **`--import-memory` + `--import-table` for cb/layout wasms** —
  the right architecture for shared state across modules.
- **`AzStartup_hydrate` as a NEW C-ABI surface** — extends the
  surface in a way that's cross-language (any binding can call
  it). Per-user direction "ship more AzStartup_* functions".
- **PLT-stub THUNK emission** (vs rewriting symbol names) — keeps
  the lifted bodies unmodified and lets wasm-ld handle linkage
  normally.
- **Server-side validation of RefAny serializer at startup
  (M8.7a)** — fail-fast for misconfigured apps.

---

## Suggested order of fixing

1. **Cross-platform symbol-size + PLT resolution.** Replace fixed
   `LIFT_READ_WINDOW` + the macOS arm64 stub parser with a
   binary-format-aware lookup (goblin or object crate). Item 4 + 5.
2. **Wire `AzStartup_dispatchEvent` to use the hydrated refany.**
   Restore the full event-dispatch chain so mousemove/keydown/etc.
   work. Items 2 + 14 + 17.
3. **Lifted `_fromJson` hydration path.** Generate a wrapper for
   the user's fromJson sig (`AzJson → AzResultRefAnyString`) and
   call it from `AzStartup_init` so user data of any shape works.
   Items 11 + 12 + 18.
4. **WASM-side hit-test + DOM diff/patch emission.** M8.5c + M8.5d.
   Items 1 + 16 + 19.
5. **Generalize wrapper signatures from api.json types.** Item 9.
6. **x86_64 + Linux support.** Items 5 + 6 (per-arch tail-call
   rewriters, ELF symbol parser, x86_64 PLT pattern).

---

End of review.
