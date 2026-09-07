# Azul — Haskell

Haskell bindings for the [Azul](https://azul.rs) GUI framework: GHC's FFI
plus a generated C shim layer, wrapped in an idiomatic `Azul` module.

## Requirements

- GHC 9.6+ and cabal 3.10+ (CI uses exactly those; `brew install ghc cabal-install`
  or [GHCup](https://www.haskell.org/ghcup/))
- the prebuilt native library (`libazul.dylib` / `libazul.so` / `azul.dll`)
  on `--extra-lib-dirs` and on the loader path at run time

## Build + run

The generated `azul` package lives next to this directory (`../azul-haskell`,
see `cabal.project`; `scripts/e2e_language_matrix.sh` puts it there from
`target/codegen/haskell/` and copies `azul.h` into its `cbits/`).

```sh
cabal build --extra-lib-dirs=$PWD/../../target/release
DYLD_LIBRARY_PATH=$PWD/../../target/release cabal run hello-world --extra-lib-dirs=$PWD/../../target/release
```

## What `HelloWorld.hs` uses

`import Azul` is the whole surface:

| Piece | Shape |
|---|---|
| `refAnyCreate :: Typeable a => a -> IO RefAny` | wrap any Haskell value as the app data |
| `refAnyGet :: Typeable a => RefAny -> IO (Maybe a)` | read it back inside a callback |
| `refAnyUpdate :: Typeable a => RefAny -> (a -> a) -> r -> IO r` | update it and answer libazul, in one expression |
| `refAnyModify :: Typeable a => RefAny -> (a -> a) -> IO ()` | update it |
| `domCreateBody`, `domCreatePWithText`, `domWithCss`, `domWithChild` | one function per api.json method, receiver last |
| `buttonCreate`, `buttonWithButtonType`, `buttonWithOnClick`, `buttonDom` | `buttonWithOnClick dat f` takes `RefAny -> CallbackInfo -> IO Update` or a pure `a -> CallbackInfo -> (a, Update)` |
| `windowCreateOptionsCreate :: LayoutCallbackHandler h => h -> IO WindowCreateOptions` | the layout: `RefAny -> LayoutCallbackInfo -> IO Dom`, or model-typed `RefAny -> a -> LayoutCallbackInfo -> IO Dom` |
| `appConfigCreate`, `appCreate`, `appRun` | `appConfigCreate >>= appCreate dat >>= appRun window` |

Because the receiver is the last argument, builder chains are `>>=`
pipelines: `domCreatePWithText "5" >>= domWithCss "font-size: 32px;"`.

## What the codegen gives you

Every helper below is emitted by a type-driven rule from api.json — no
method-name allowlist, no hand-written wrappers.

- **`Azul`** (`src/Azul.hs`): one managed wrapper type per resource-owning
  class (`data Dom = Dom { domRaw :: ForeignPtr T.Dom, domConsumed :: IORef Bool }`)
  with `allocDom` / `borrowDom` / `withDom` / `consumeDom` / `disposeDom`
  / `domFromValue` / `domToValue`; one function per api.json constructor
  and method; `Show` / `Eq` instances routed through `_toDbgString` /
  `_partialEq` where api.json derives them; the host-handle `RefAny`;
  `<Kind>Fn` closure types, a `<Kind>Handler` class (raw closure or
  model-typed function) and a registered invoker for every callback
  kind in `HOST_INVOKER_KINDS`; re-exports of every `Azul.Types` enum and
  plain struct, so `Update_RefreshDom` and `ButtonType_Primary` are in
  scope.
- **`Azul.Types`** (`src/Azul/Types.hs`): `data` declarations mirroring the C
  structs, enums and tagged unions. Every `Storable` instance takes its
  `sizeOf`, `alignment` and member offsets from the cbits layout oracle
  (`az_hs_sizeof_<T>` / `az_hs_alignof_<T>` / `az_hs_offsetof_<T>_<m>`,
  i.e. the C compiler's numbers for `azul.h`), so no size is ever written
  down by hand. `<vec>ToList`, `azStringToString`, `encodeUtf8` /
  `decodeUtf8` and the `<option>IsSome` / `<result>IsOk` tag readers live
  here too.
- **`Azul.Internal.FFI`** (`src/Azul/Internal/FFI.hs`): the raw
  `foreign import`s (`c_Az<X>` / `c_Az<X>_via` for struct-by-value
  signatures), the host-invoker protocol imports, and the
  `register<X>Callback` trampoline helpers for callback kinds without a
  host invoker. Every import is `safe`: a host-handle `RefAny` destructor
  re-enters Haskell through the releaser.
- **`cbits/azul_shims.c`**: the `_via` shims, the inbound trampolines, the
  host-invoker prototypes and the layout oracle.

## Ownership

A by-value C parameter *moves* a wrapper's bytes into libazul; the
generated function marks the wrapper consumed (`consumeDom`) and the
`ForeignPtr` buffer is freed by the GC. Values you never hand over can be
released explicitly with `disposeDom`; nothing is ever deleted from a
finalizer. `RefAny` arguments are passed by clone, so the caller keeps its
own; `refAnyCreate`'s table entry is released when the last clone drops.

Callbacks fire on the thread that runs `appRun`, through `foreign import
"wrapper"` FunPtrs; build executables `-threaded` (the example does) so a
release from another thread cannot abort the runtime.
