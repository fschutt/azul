# Azul — Haskell

Haskell bindings for the [Azul](https://azul.rs) GUI framework via
GHC's FFI + a C-shim layer for struct-by-value plumbing.

## Status

🟡 **Inbound + outbound shim layers complete; full App.run blocked
on libazul macOS webrender crash.** The smoke test exercises both
shim directions:

- Outbound (`<name>_via`): Haskell calls libazul with struct-by-value
  args / returns through pointer-wrapped wrappers.
- Inbound (`<name>_trampoline` + `<name>_set_inner`): libazul calls
  Haskell back through a C trampoline that has the C-ABI by-value
  signature; the trampoline forwards to a Haskell-friendly inner
  (`mk_<name>_inner`) with by-pointer args + out-pointer return.

AZ_DEBUG 5→8 verification is blocked by the macOS-side libazul
webrender bug (same as Pascal — see `memory/pascal_codegen_2026_05_13.md`).

## Requirements

- GHC 9.x (`brew install ghc`)
- cabal 3.x
- `libazul.dylib` in `../azul-haskell/` or on `extra-lib-dirs`

## Build + Run

```sh
cabal build
DYLD_LIBRARY_PATH=. cabal run hello-world
```

## Architecture: the `_via` and `_trampoline` shim layers

GHC's `foreign import ccall` doesn't support passing or returning C
structs by value. Two complementary shim families handle the two
directions:

### Outbound — `<name>_via`

For every `azul.h` function whose C-ABI signature uses a by-value
aggregate, the codegen emits:

```c
void AzDom_createBody_via(AzDom *__out) {
    *__out = AzDom_createBody();
}
```

Haskell side: `alloca` a buffer, call the `_via` form, `peek` the
result. The shim takes by-value aggregate args as `const T *` and
writes by-value returns through a trailing `T *__out`.

### Inbound — `<name>_trampoline` + `<name>_set_inner`

For every callback typedef (e.g. `LayoutCallbackType`), the codegen
emits a trampoline that matches the C ABI's by-value-struct signature
and forwards to a Haskell-friendly inner:

```c
typedef void (*AzLayoutCallbackType_inner)(const AzRefAny *,
                                           const AzLayoutCallbackInfo *,
                                           AzDom *__out);
static AzLayoutCallbackType_inner g_AzLayoutCallbackType_inner = 0;
void AzLayoutCallbackType_set_inner(AzLayoutCallbackType_inner f) {
    g_AzLayoutCallbackType_inner = f;
}
AzDom AzLayoutCallbackType_trampoline(AzRefAny r, AzLayoutCallbackInfo info) {
    AzDom __ret;
    if (g_AzLayoutCallbackType_inner)
        g_AzLayoutCallbackType_inner(&r, &info, &__ret);
    return __ret;
}
```

Haskell side: wrap a `Ptr RefAny -> Ptr LayoutCallbackInfo -> Ptr Dom -> IO ()`
fn via `mk_LayoutCallbackType_inner`, register the FunPtr via
`c_AzLayoutCallbackType_set_inner`, then splice
`p_AzLayoutCallbackType_trampoline` (a `FunPtr ()` pointing at the
trampoline) into the WCO's `layout_callback` field as the actual
C-ABI fn pointer libazul calls through.

### Why two layers?

GHC's `foreign import ccall "wrapper"` produces a fn pointer with a
fixed shape that doesn't match the C ABI for struct-by-value returns
(>16 bytes triggers sret on AArch64). The trampoline-with-static-slot
pattern bridges this mismatch with one extra indirection per call.

The current implementation uses a single static slot per callback
typedef — fine for single-window apps; multi-window or multi-instance
callbacks would need a handle table on the trampoline side.

## Files

- `HelloWorld.hs` — smoke test for both shim directions.
- `azul-example.cabal` — example executable manifest.
- `cabal.project` — points at `../azul-haskell` for the in-tree
  `azul` library package.
- `libazul.dylib` — prebuilt native library.

The library proper lives in `../azul-haskell/`:

- `src/Azul.hs` — umbrella module.
- `src/Azul/Types.hs` — Storable instances mirroring the C structs.
- `src/Azul/Internal/FFI.hs` — `foreign import` declarations, with
  per-callback-typedef `mk_<X>_inner` + `c_Az<X>_set_inner` +
  `p_Az<X>_trampoline` triplets.
- `cbits/azul_shims.c` — outbound `_via` shims + inbound
  `_trampoline` / `_set_inner` plumbing.
- `cbits/azul.h` — generated header (copy of `target/codegen/azul.h`).

## Next steps

- Splice `p_AzLayoutCallbackType_trampoline` into a populated
  `WindowCreateOptions` struct (needs Storable-poke offsets for
  `window_state.layout_callback`).
- Full `App.run` driven by the trampoline.
- AZ_DEBUG counter probe verification once libazul's macOS webrender
  crash is resolved (see `memory/pascal_codegen_2026_05_13.md` — same
  blocker as Pascal).
- A `MyDataModel -> Dom`-style API on top of `Azul.hs` so user code
  doesn't see the trampoline machinery.
