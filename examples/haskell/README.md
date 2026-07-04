# Azul — Haskell

Haskell bindings for the [Azul](https://azul.rs) GUI framework via
GHC's FFI + a per-direction C-shim layer + codegen-driven idiomatic
wrappers.

## Status

🟢 **Full GUI counter E2E passing** (2026-07-04): `HelloWorld.hs` is a
complete counter app (body > div{font-size:32px} > text + "Increase
counter" button). With `AZ_E2E=tests/e2e/hello_world_counter.json` and
`AZ_BACKEND=headless` the libazul runner clicks the button and prints
`test result: ok` — `bash scripts/e2e_language_matrix.sh haskell`
reports `✓ WORKS`. The former macOS webrender blocker (C.1) was fixed
libazul-side on 2026-07-03.

## Requirements

- GHC 9.x (`brew install ghc`)
- cabal 3.x
- `libazul.dylib` next to the package (or on `extra-lib-dirs`)

## Build + Run

```sh
cabal build
DYLD_LIBRARY_PATH=. cabal run hello-world
```

## What the codegen gives you

Generated from a single api.json IR pass — no hand-written wrappers.
Every helper below is emitted by a type-driven rule (no method-name
allowlist).

| Helper | Source rule | Example |
|---|---|---|
| `register<X>Callback` | every `ir.callback_typedefs` entry | `registerLayoutCallbackTypeCallback :: (Ptr (RefAny ()) -> Ptr LayoutCallbackInfo -> IO Dom) -> IO (FunPtr ())` |
| `<lower>VecToList` | struct fields exactly `[ptr, len, cap, destructor]` | `domVecToList :: DomVec -> IO [Dom]` |
| `azStringToString` | `TypeCategory::String` | `azStringToString :: AzString -> IO String` |
| `<lower>IsNone` / `IsSome` | enum variants exactly `[None, Some]` | `optionI16IsSome :: Ptr OptionI16 -> IO Bool` |
| `<lower>IsOk` / `IsErr` | enum variants exactly `[Ok, Err]` | `resultI32CompileErrorIsOk :: Ptr ResultI32CompileError -> IO Bool` |
| `instance Show <X>` | `TypeTraits.is_debug` AND `_toDbgString_via` exported | `show (Dom p) = ...` routes through `c_AzDom_toDbgString_via` |
| `instance Eq <X>` | `TypeTraits.is_partial_eq` AND `_partialEq` exported | `(Dom a) == (Dom b) = ...` routes through `c_AzDom_partialEq` |

## Architecture: two shim layers

### Outbound — `<name>_via`

For every `azul.h` function whose C-ABI signature uses a by-value
aggregate, the codegen emits a shim that takes aggregate args as
`const T *` and writes aggregate returns through a trailing `T *__out`:

```c
void AzDom_createBody_via(AzDom *__out) { *__out = AzDom_createBody(); }
```

Haskell side: `alloca` a buffer, call the `_via` form, `peek` the
result.

### Inbound — `<name>_trampoline` + `<name>_set_inner`

For every callback typedef, the codegen emits a trampoline matching
the C ABI's by-value-struct signature, plus a setter for a Haskell
inner FunPtr:

```c
typedef void (*AzLayoutCallbackType_inner)(
  const AzRefAny *, const AzLayoutCallbackInfo *, AzDom *__out);

static AzLayoutCallbackType_inner g_AzLayoutCallbackType_inner = 0;
void AzLayoutCallbackType_set_inner(AzLayoutCallbackType_inner f) { ... }
AzDom AzLayoutCallbackType_trampoline(AzRefAny r, AzLayoutCallbackInfo i) {
    AzDom __ret;
    if (g_AzLayoutCallbackType_inner)
        g_AzLayoutCallbackType_inner(&r, &i, &__ret);
    return __ret;
}
```

The `register<X>Callback` helper hides this triplet behind a single
Haskell function. User writes `MyData -> Info -> IO Dom`; helper
returns a `FunPtr ()` to splice into a `WindowCreateOptions`.

### Why two layers?

GHC's `foreign import ccall` doesn't support struct-by-value across
the boundary — neither as args nor returns. The outbound `_via` shims
let Haskell call libazul with aggregates through pointer wrappers. The
inbound `_trampoline` shims let libazul call Haskell back through a C
fn with the right by-value-struct signature, forwarding to a Haskell-
friendly out-pointer inner.

The current implementation uses a single static slot per callback
typedef. Multi-callback or multi-window apps would need a handle table
on the trampoline side.

## Files

- `HelloWorld.hs` — full-GUI counter hello-world (~150 LOC). Installs
  the layout + typed ButtonOnClick trampolines via `mk_<X>_inner` /
  `set_inner`, keeps app state in a Haskell `IORef` captured by the
  inner closures, and builds the DOM with the raw `c_Az*_via`
  out-pointer primitives (see the note in the file header on why the
  DOM is not round-tripped through `T.Dom` values).
- `azul-example.cabal` — example executable manifest.
- `cabal.project` — points at `../azul-haskell/` for the in-tree
  `azul` library package.
- `libazul.dylib` — prebuilt native library.

The library lives in `../azul-haskell/`:

- `src/Azul.hs` — umbrella module with `withFoo` brackets and
  type-driven `Show` / `Eq` instances routed through C-ABI helpers.
- `src/Azul/Types.hs` — Storable instances mirroring the C structs,
  per-Vec `<lower>VecToList` helpers, per-Option/Result tag-byte
  accessors, AzString `<lower>ToString` decoder.
- `src/Azul/Internal/FFI.hs` — raw `foreign import` declarations,
  per-callback-typedef `mk_<X>_inner` / `c_Az<X>_set_inner` /
  `p_Az<X>_trampoline` triplets, and the user-facing
  `register<X>Callback` helpers that hide them.
- `cbits/azul_shims.c` — outbound `_via` shims + inbound
  `_trampoline` / `_set_inner` plumbing.
- `cbits/azul.h` — generated header (copy of `target/codegen/azul.h`).

## Full App.run wiring (H.2 — DONE 2026-07-04)

No Storable-offset splice is needed: `AzWindowCreateOptions_create`
takes the layout callback as its argument, so the example simply pokes
`p_AzLayoutCallbackType_trampoline` into a `FunPtr`-sized cell and
calls `c_AzWindowCreateOptions_create_via`. Two rules keep the run
loop stable:

1. `c_AzApp_run_via` must be a **`safe`** foreign import (patched in
   the generated `FFI.hs`): libazul re-enters Haskell through the
   trampolines while `AzApp_run` is on the C side, and call-ins during
   an `unsafe` call abort/deadlock the GHC RTS. The example executable
   is also built `-threaded` for robust foreign call-ins.
2. Aggregate out-buffers are sized from the C ABI (`sizeof()` of
   `azul.h` structs), not from the Haskell `Storable` instances —
   tagged-union placeholder sizes in `Types.hs` are estimates and
   their `peek`/`poke` intentionally `error` out, so `T.Dom` values
   must never be peeked; build DOM bytes with `_via` calls instead.

## Recent updates (2026-05-15/16)

- **R12 consume mechanism** (commit `634ff5de2`): two-field
  `data Foo = Foo (Ptr T.Foo) (IORef Bool)` with `consumeFoo` /
  `withFoo` / `disposeFoo` helpers. The bracket release reads the
  IORef tombstone before firing `_delete`, so consuming-self calls
  no longer double-free on scope exit.
- **V8 Vec iter clone-via** (commit `e73ab429c`): `<lower>VecToList`
  now clones each element via `Az<X>_clone_via` (the Phase-B.8 shim
  layer) when available, instead of shallow `peekElemOff`. List
  entries survive the Vec being closed. POD elements without
  `_clone` fall back to the legacy path with a warning comment.
- **Per-method emit layer**: investigated 2026-05-16 and re-queued
  as item 18 in HANDOFF_2026_05_16.md per user direction. ~4-6h
  focused codegen work to build `Azul.addChild :: Dom -> Dom ->
  IO ()` wrappers over the raw `c_AzDom_addChild` FFI imports,
  with auto-`consume<X>` on Owned args. Implementation plan in
  `memory/haskell_consume_per_method_investigation_2026_05_16.md`.
