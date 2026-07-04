---
slug: hello-world/haskell
title: Hello World [Haskell]
language: en
canonical_slug: hello-world/haskell
audience: external
maturity: wip
guide_order: 27
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/haskell/HelloWorld.hs
  - examples/haskell/azul-example.cabal
last_generated_rev: dab922c5e869ab3c1ff69a2d7f4af1af19a5c27c
generated_at: 2026-07-04T00:00:00Z
default-search-keys:
  - App
  - AppConfig
  - Dom
  - Button
  - RefAny
  - WindowCreateOptions
  - Update
---

# Hello World [Haskell]

## Introduction

The Haskell binding drives the prebuilt `libazul` native library
through GHC's FFI plus a generated C shim layer. GHC's
`foreign import` cannot pass or return C structs *by value*, so the
binding ships `cbits/azul_shims.c`: for every `azul.h` function with a
by-value aggregate in its signature there is a `<name>_via` shim that
takes aggregates behind pointers and writes returns through a trailing
out-pointer. Callbacks go the other direction through per-kind
**trampolines**: the shim holds one static C function per callback
typedef (`Az<X>Type_trampoline`) plus a global *inner* slot
(`Az<X>Type_set_inner`); you wrap a Haskell closure into a `FunPtr`
with the codegen's `mk_<X>_inner`, store it in the slot, and hand
libazul the trampoline's address.

The generated package has three modules:

- **`Azul.Types`** — `Storable` struct definitions mirroring `azul.h`.
- **`Azul.Internal.FFI`** — the raw `c_Az*_via` imports plus the
  trampoline machinery (`mk_<X>_inner`, `c_Az<X>Type_set_inner`,
  `p_Az<X>Type_trampoline`). The counter below works at this level.
- **`Azul`** — curated wrappers (`with*`/`dispose*` RAII brackets,
  `Maybe`/`Either` decoding, `Show`/`Eq` instances).

Because `AzApp_run` re-enters Haskell (every layout and click
trampoline fires while the event loop is on the C side), it is imported
`ccall safe` and the example executable builds with `-threaded` — the
downloaded `azul-example.cabal` already carries that flag.

## Installation

You need **GHC 9.x** and **cabal 3.x** (via [GHCup](https://www.haskell.org/ghcup/);
on Windows use GHCup under MSYS2). Two packages sit side by side: the
generated `azul` library and your executable. Keep them in *separate*
directories — cabal refuses two `.cabal` files in one directory.

```sh
# 1. the generated azul library package -> ./azul-haskell/
mkdir -p azul-haskell/src/Azul/Internal azul-haskell/cbits

# native library: pick your platform
curl -o azul-haskell/libazul.so    https://azul.rs/ui/release/$VERSION/libazul.so     # linux
curl -o azul-haskell/libazul.dylib https://azul.rs/ui/release/$VERSION/libazul.dylib  # macOS
curl -o azul-haskell/azul.dll      https://azul.rs/ui/release/$VERSION/azul.dll       # windows

curl -o azul-haskell/azul.cabal              https://azul.rs/ui/release/$VERSION/azul.cabal
curl -o azul-haskell/src/Azul.hs             https://azul.rs/ui/release/$VERSION/Azul.hs
curl -o azul-haskell/src/Azul/Types.hs       https://azul.rs/ui/release/$VERSION/Azul/Types.hs
curl -o azul-haskell/src/Azul/Internal/FFI.hs https://azul.rs/ui/release/$VERSION/Azul/Internal/FFI.hs

# the C shim layer that azul.cabal compiles (c-sources + include-dirs: cbits)
curl -o azul-haskell/cbits/azul_shims.c https://azul.rs/ui/release/$VERSION/azul_shims.c
curl -o azul-haskell/cbits/azul.h       https://azul.rs/ui/release/$VERSION/azul.h

# 2. the example executable package -> ./
curl -O https://azul.rs/ui/release/$VERSION/azul-example.cabal
curl -O https://azul.rs/ui/release/$VERSION/HelloWorld.hs
```

Add a two-line `cabal.project` next to `azul-example.cabal` so cabal
finds the in-tree `azul` package (it is not on Hackage):

```
packages: .
          ./azul-haskell
```

Then build and run (the `--extra-lib-dirs` must be an *absolute* path —
ghc-pkg rejects relative ones during registration):

```sh
cabal build --extra-lib-dirs=$PWD/azul-haskell

# linux
LD_LIBRARY_PATH=$PWD/azul-haskell cabal run hello-world --extra-lib-dirs=$PWD/azul-haskell
# macOS
DYLD_LIBRARY_PATH=$PWD/azul-haskell cabal run hello-world --extra-lib-dirs=$PWD/azul-haskell
# windows (azul.dll must be on PATH)
set PATH=%CD%\azul-haskell;%PATH%
cabal run hello-world
```

The first `cabal build` compiles the full generated binding (three
multi-megabyte modules plus the C shim) and takes several minutes;
subsequent builds come from the cache.

## Simple "Counter" Example

This is the complete, verified `HelloWorld.hs` (the same file the
install step downloads):

```haskell
{-# LANGUAGE ScopedTypeVariables #-}
-- cabal run hello-world
--
-- Full-GUI counter hello-world for the Azul Haskell bindings.
--
-- DOM shape (matches tests/e2e/hello_world_counter.json):
--   body
--   ├── div { font-size: 32px }
--   │   └── text("5")            -- the counter, increments on click
--   └── Button "Increase counter"
--
-- Callback mechanism: the Haskell binding uses inbound TRAMPOLINES.
-- cbits/azul_shims.c holds one static C function per callback typedef
-- (Az<X>Type_trampoline) plus a global "inner" slot (Az<X>Type_set_inner).
-- We create a Haskell FunPtr with the codegen's `mk_<X>_inner` wrapper,
-- store it in the slot, and hand libazul the trampoline's address.
-- Because the inner is a Haskell *closure*, application state lives in a
-- plain IORef captured by the closures — no downcast through RefAny needed
-- (the RefAny we pass around is a zero-sized placeholder that satisfies
-- libazul's refcounting).
--
-- NOTE on style: the DOM is built exclusively with the raw `c_Az*_via`
-- out-pointer primitives (writing straight into C-provided buffers).
-- The generated struct Storables for DOM-sized aggregates contain
-- tagged-union placeholders whose peek/poke intentionally `error` out, so
-- value-level round-trips through `T.Dom` must be avoided.

module Main where

import Azul.Internal.FFI
import qualified Azul.Types as T
import Data.IORef
import Foreign.C.String (withCAStringLen)
import Foreign.C.Types (CSize)
import Foreign.Marshal.Alloc (alloca, allocaBytes, mallocBytes)
import Foreign.Marshal.Utils (fillBytes)
import Foreign.Ptr (Ptr, FunPtr, castPtr)
import Foreign.Storable (poke)

-- C ABI sizes (checked against sizeof() of the shipped azul.h).
-- Kept generous where the exact size could drift.
szRefAny, szString, szDom, szButton, szWco, szAppConfig, szApp, szOnClickCb :: Int
szRefAny    = 32    -- sizeof(AzRefAny)  = 24
szString    = 48    -- sizeof(AzString)  = 40
szDom       = 512   -- sizeof(AzDom)     = 240
szButton    = 512   -- sizeof(AzButton)  = 272
szWco       = 2048  -- sizeof(AzWindowCreateOptions) = 1336
szAppConfig = 2048  -- sizeof(AzAppConfig) = 1648
szApp       = 64    -- sizeof(AzApp)     = 16
szOnClickCb = 64    -- sizeof(AzButtonOnClickCallback) = 40 (cb + OptionRefAny::None)

-- | Write an owned AzString (copied from a Haskell String) into @out@.
-- ASCII-only content in this example, so the Latin-1 marshalling is
-- valid UTF-8.
mkAzString :: String -> Ptr T.AzString -> IO ()
mkAzString s out =
  withCAStringLen s $ \(p, len) ->
    c_AzString_copyFromBytes_via (castPtr p) 0 (fromIntegral len :: CSize) out

-- | Build a zero-sized placeholder RefAny. State does NOT live here — it
-- lives in the IORef captured by the callback closures. libazul still
-- clones/drops this, so it must be a real refcounted RefAny.
mkPlaceholderRefAny :: FunPtr () -> Ptr (T.RefAny ()) -> IO ()
mkPlaceholderRefAny dtorTramp out =
  allocaBytes 16 $ \(gvp :: Ptr T.GlVoidPtrConst) -> do
    fillBytes gvp 0 16                    -- { ptr = NULL, run_destructor = false }
    allocaBytes szString $ \typeName -> do
      mkAzString "HsCounterModel" typeName
      alloca $ \(dtorCell :: Ptr (FunPtr ())) -> do
        poke dtorCell dtorTramp
        c_AzRefAny_newC_via gvp 0 1 0xBA5EBA11 typeName (castPtr dtorCell) 0 0 out

-- | The layout function: rebuilds the whole DOM from the counter value.
-- Writes the resulting AzDom directly into the trampoline's out-pointer.
buildLayout :: IORef Int
            -> Ptr (T.RefAny ())            -- master placeholder RefAny (cloned per button)
            -> Ptr T.ButtonOnClickCallback  -- prepared { cb = trampoline, callable = None }
            -> Ptr (T.RefAny ()) -> Ptr T.LayoutCallbackInfo -> Ptr T.Dom -> IO ()
buildLayout counter master clickCb _data _info outPtr = do
  n <- readIORef counter
  c_AzDom_createBody_via outPtr

  -- div { font-size: 32px } > text(show n)
  allocaBytes szDom $ \divBuf -> do
    c_AzDom_createDiv_via divBuf
    allocaBytes szString $ \css -> do
      mkAzString "font-size: 32px;" css
      c_AzDom_setCss_via divBuf css       -- consumes css
    allocaBytes szDom $ \txt ->
      allocaBytes szString $ \label -> do
        mkAzString (show n) label
        c_AzDom_createText_via label txt  -- consumes label
        c_AzDom_addChild_via divBuf txt   -- consumes txt
    c_AzDom_addChild_via outPtr divBuf    -- consumes divBuf

  -- Button "Increase counter" (typed ButtonOnClick callback)
  allocaBytes szButton $ \btn -> do
    allocaBytes szString $ \label -> do
      mkAzString "Increase counter" label
      c_AzButton_create_via label btn     -- consumes label
    alloca $ \(btnType :: Ptr T.ButtonType) -> do
      poke btnType T.ButtonType_Primary
      c_AzButton_setButtonType_via btn btnType
    allocaBytes szRefAny $ \dataClone -> do
      c_AzRefAny_clone_via master dataClone
      c_AzButton_setOnClick_via btn dataClone clickCb  -- consumes dataClone
    allocaBytes szDom $ \btnDom -> do
      c_AzButton_dom_via btn btnDom       -- consumes btn
      c_AzDom_addChild_via outPtr btnDom  -- consumes btnDom

main :: IO ()
main = do
  putStrLn "[azul] Haskell hello-world (counter) starting."

  counter <- newIORef (5 :: Int)

  -- 1. No-op RefAny destructor (payload is zero-sized; nothing to free).
  dtorInner <- mk_RefAnyDestructorType_inner (\_ -> pure ())
  c_AzRefAnyDestructorType_set_inner dtorInner

  -- 2. Master placeholder RefAny (lives for the program's lifetime).
  master <- mallocBytes szRefAny :: IO (Ptr (T.RefAny ()))
  mkPlaceholderRefAny p_AzRefAnyDestructorType_trampoline master

  -- 3. Typed button on-click: increment the IORef, request a re-layout.
  clickInner <- mk_ButtonOnClickCallbackType_inner $ \_data _info out -> do
    modifyIORef' counter (+ 1)
    poke out T.Update_RefreshDom
  c_AzButtonOnClickCallbackType_set_inner clickInner
  -- AzButtonOnClickCallback value = { cb = trampoline, callable = None(0) }.
  clickCb <- mallocBytes szOnClickCb :: IO (Ptr T.ButtonOnClickCallback)
  fillBytes clickCb 0 szOnClickCb
  poke (castPtr clickCb :: Ptr (FunPtr ())) p_AzButtonOnClickCallbackType_trampoline

  -- 4. Layout callback: builds the DOM into the trampoline's out-pointer.
  layoutInner <- mk_LayoutCallbackType_inner (buildLayout counter master clickCb)
  c_AzLayoutCallbackType_set_inner layoutInner

  -- 5. WindowCreateOptions(layout_callback) + AppConfig + App, then run.
  allocaBytes szWco $ \wco -> do
    alloca $ \(cbCell :: Ptr (FunPtr ())) -> do
      poke cbCell p_AzLayoutCallbackType_trampoline
      c_AzWindowCreateOptions_create_via (castPtr cbCell) wco
    allocaBytes szAppConfig $ \cfg -> do
      c_AzAppConfig_create_via cfg
      allocaBytes szRefAny $ \appData -> do
        c_AzRefAny_clone_via master appData
        allocaBytes szApp $ \app -> do
          c_AzApp_create_via appData cfg app  -- consumes appData + cfg
          c_AzApp_run_via app wco             -- consumes wco; blocks until exit

  putStrLn "[azul] App exited cleanly."
```

Five things to notice.

- **Trampolines + inner slots.** Each callback kind
  (`LayoutCallbackType`, `ButtonOnClickCallbackType`,
  `RefAnyDestructorType`, ...) has a fixed C trampoline in the shim and
  one global inner slot. `mk_<X>_inner` wraps your Haskell closure into
  a `FunPtr`, `c_Az<X>Type_set_inner` stores it, and
  `p_Az<X>Type_trampoline` is the address you hand libazul — spliced
  into `WindowCreateOptions_create` for layout, and poked at offset 0
  of the `AzButtonOnClickCallback` cell for the button. One slot per
  kind: registering a second `ButtonOnClick` closure rebinds *all*
  buttons to the newest one.
- **Why not the `register<Kind>Callback` helper?** The generated
  convenience helper (`registerLayoutCallbackTypeCallback` and
  friends) does the same `set_inner` dance, but its signature returns
  your `Dom` *by value*, which forces a peek/poke round-trip through
  `T.Dom` — and the `Storable` instances for tagged-union types
  deliberately fail (`error "... tagged-union peek not implemented;
  use the raw FFI primitives"`) because Haskell cannot model the C
  unions' exact layout yet. So the example calls
  `mk_LayoutCallbackType_inner` directly and builds the DOM straight
  into the C-provided out-pointer with the raw `_via` primitives.
  Avoid value-level round-trips through DOM-sized aggregates.
- **State lives in Haskell closures.** The `IORef` is captured by both
  the layout and the click inner — no marshalling of the model through
  the `RefAny` is needed. The `RefAny` the API requires is a zero-sized
  placeholder (`AzRefAny_newC` with a NULL/0 payload and a no-op
  destructor trampoline); it is still a *real* refcounted RefAny that
  libazul clones and drops, which is why the master cell and the
  prepared `clickCb` struct are `mallocBytes`'d once and live for the
  program's lifetime.
- **Buffer sizes come from C `sizeof()`, not `Storable`.** Buffers that
  receive an `Az*` struct by value are `allocaBytes`'d with sizes
  checked against the shipped `azul.h` (kept deliberately generous —
  see the `sz*` constants). Do not size them with `alloca` /
  `Storable` `sizeOf`: the Haskell sizes underestimate padded and
  union-carrying structs (`RefAny`'s `Storable` is pointer-sized, the
  C struct is 24 bytes).
- **Ownership is a one-way move, and the event loop is a `safe` call.**
  By-value parameters are *consumed* by the C side (it copies the bytes
  and takes ownership) — the `-- consumes ...` comments track this;
  temporaries may die right after the call and must not also be
  deleted by you. Because `AzApp_run` re-enters Haskell through the
  trampolines, the binding imports it `ccall safe` and the example
  builds with `-threaded`.

## Build and run

```sh
cabal build --extra-lib-dirs=$PWD/azul-haskell
DYLD_LIBRARY_PATH=$PWD/azul-haskell cabal run hello-world --extra-lib-dirs=$PWD/azul-haskell
```

You should see the window pictured on the
[hello-world landing page](../hello-world.md): the label renders "5";
each click runs the click inner, bumps the `IORef`, pokes
`Update_RefreshDom` through the out-pointer, and the framework re-runs
the layout closure with the new value.

## Common errors

- **`cabal: Multiple cabal files found`** — `azul.cabal` and
  `azul-example.cabal` ended up in the same directory. Keep the
  generated library in its own subdirectory and point `cabal.project`
  at it.
- **C shim fails with `azul.h: No such file or directory`** — the
  header must sit *inside* `azul-haskell/cbits/` (the package compiles
  `cbits/azul_shims.c` with `include-dirs: cbits`). Download it there,
  not just to the project root.
- **Link error `cannot find -lazul`** — `--extra-lib-dirs` missing or
  relative. Pass an absolute path (`$PWD/azul-haskell`).
- **Runtime: `error while loading shared libraries` / `dyld: Library
  not loaded`** — set `LD_LIBRARY_PATH` (Linux) or `DYLD_LIBRARY_PATH`
  (macOS) to the directory containing the native library; on Windows
  put `azul.dll` on `PATH`.
- **`error: ... tagged-union peek not implemented; use the raw FFI
  primitives`** — you round-tripped a DOM-sized aggregate through its
  Haskell value (a `peek` on `Ptr T.Dom`, or the generated
  `registerLayoutCallbackTypeCallback` helper, which does that
  internally). Build such structs straight into the out-pointer with
  the raw `c_Az*_via` primitives, as the example does.
- **Corrupted state or crashes after a `_via` call** — an out-buffer
  was sized with `alloca` / `Storable` `sizeOf` instead of
  `allocaBytes` with the real C `sizeof()`. Use the `sz*` constants
  pattern from the example.
- **Unsafe foreign import + callback re-entry aborts the GHC RTS** —
  the shipped binding imports `AzApp_run` as `safe`; if you write your
  own imports, mark anything that can re-enter Haskell (the event
  loop, timers, threads) as `safe`, and build with `-threaded`.
- **All buttons suddenly run the same handler** — you stored a second
  `ButtonOnClickCallbackType` inner; the newest `set_inner` wins for
  the whole kind. Register one closure and dispatch inside it.
- **Counter does not advance** — the click inner did not poke
  `T.Update_RefreshDom` through its out-pointer (leaving it unwritten
  is undefined — always poke an `Update`), or you spliced the *layout*
  trampoline into the button cell by mistake (both are `FunPtr ()` —
  keep the names distinct).
- **App freezes or crashes after a Haskell exception in a callback** —
  exceptions must not escape an inner closure (they would unwind
  through C and Rust frames). Wrap handler bodies in
  `Control.Exception.try` and poke `T.Update_DoNothing` on the error
  path.
- **First build takes minutes / lots of memory** — expected: the
  generated `Azul.Types` / `Azul.Internal.FFI` modules are several MB
  of Haskell each. Later builds are incremental.

## Coming Up Next

- [Application Architecture](../architecture.md) — architecting a larger Azul application
- [Document Object Model](../dom.md) — the Dom tree: node types, hierarchy, and CSS
- [Hello World [C]](c.md)
