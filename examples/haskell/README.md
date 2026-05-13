# Azul — Haskell

Haskell bindings for the [Azul](https://azul.rs) GUI framework via
GHC's FFI + a C-shim layer for struct-by-value returns.

## Status

🟡 **Host-invoker smoke layer with C-shim plumbing**. The smoke test
now exercises the `<name>_via` shim path that lets GHC's FFI marshal
struct-by-value through out-pointers. Full App.run + layout callbacks
are the next codegen pass; the shim foundation is in place.

## Requirements

- GHC 9.x (`brew install ghc`)
- cabal 3.x
- `libazul.dylib` in the package root or on `extra-lib-dirs`

## Build + Run

```sh
cabal build
DYLD_LIBRARY_PATH=. cabal run hello-world
```

## Architecture: the `_via` shim layer

GHC's `foreign import ccall` doesn't support passing or returning C
structs by value. Every function in `azul.h` whose C-ABI signature
uses one — and that's most of them, including `AzDom_createBody`,
`AzApp_create`, `AzWindowCreateOptions_default` — therefore can't be
called directly from Haskell.

The codegen emits a small C shim per such function:

```c
void AzDom_createBody_via(AzDom *__out) {
    *__out = AzDom_createBody();
}
```

The shim takes by-value aggregate args as `const T *` and writes
by-value returns through a trailing `T *__out`. cabal compiles
`cbits/azul_shims.c` into the library; the Haskell foreign-import
points at the `_via` symbol; on the Haskell side the user
`alloca`s a buffer, calls the `_via` form, and peeks the result.

The smoke test in `HelloWorld.hs` demonstrates both paths:
- Direct `c_AzString_delete` (pointer arg, void return — no shim needed).
- `c_AzDom_createBody_via` (struct-by-value return — uses the shim).

## Files

- `HelloWorld.hs` — smoke test with `_via` round-trip.
- `azul-example.cabal` — example executable manifest.
- `cabal.project` — points at `../azul-haskell` for the in-tree
  `azul` library package.
- `libazul.dylib` — prebuilt native library.

The library proper lives in `../azul-haskell/`:
- `src/Azul.hs` — umbrella module.
- `src/Azul/Types.hs` — Storable instances mirroring the C structs.
- `src/Azul/Internal/FFI.hs` — `foreign import` declarations.
- `cbits/azul_shims.c` — the C-shim layer.
- `cbits/azul.h` — generated header (copy of `target/codegen/azul.h`).

## Next steps

- Full App.run wiring + callback trampolines (`foreign import ccall "wrapper"`).
- A `Layout :: MyDataModel -> Dom` style entry point on top of the
  generated `Azul` module (per the architecture-alignment block in
  `Azul.hs`).
- AZ_DEBUG counter probe verification.
