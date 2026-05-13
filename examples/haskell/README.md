# Azul — Haskell

Haskell bindings for the [Azul](https://azul.rs) GUI framework via
GHC's FFI + a per-direction C-shim layer + codegen-driven idiomatic
wrappers.

## Status

🟢 **Codegen-side polished**: Phase H — outbound + inbound shim
layers, register helpers, Show/Eq routing, Vec→list, Option/Result
tag-byte, AzString round-trip — all land in the generator. AZ_DEBUG
full-GUI verification is blocked at the libazul macOS webrender side
(C.1, same blocker as Pascal/Lisp).

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

- `HelloWorld.hs` — Python-quality smoke test (~64 LOC).
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

## Status of full App.run (H.2)

Splicing the trampoline `FunPtr ()` into `WindowCreateOptions`'s
nested `window_state.layout_callback` field needs platform-aware
Storable-offset arithmetic the codegen doesn't carry today (the offset
depends on the exact field layout of WindowState). The pieces are in
place; the splice is one focused task once libazul's macOS event-loop
crash (C.1) clears and the codegen exposes the offset.
