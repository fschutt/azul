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
last_generated_rev: 88ad8e70f5de6612d619f7e59ea5f9fe04ae4a8e
generated_at: 2026-09-08T00:00:00Z
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

The Haskell binding drives the prebuilt `libazul` native library through
GHC's FFI. You write ordinary Haskell: a model type, a layout function
that builds the DOM from it, and a click handler that updates the model
with a pure function. The generated `azul` package does the rest — it
wraps your model in a libazul `RefAny`, turns your functions into
callbacks libazul can invoke, and marshals every struct through the C ABI
with sizes and offsets the C compiler computed.

`import Azul` is the whole surface. It exports one managed wrapper type per
resource-owning class (`Dom`, `Button`, `App`, ...), one function per
api.json constructor and method, and every enum of `Azul.Types`, so
`ButtonType_Primary` and `Update_RefreshDom` are in scope.

## Installation

You need **GHC 9.6+** and **cabal 3.10+** (via [GHCup](https://www.haskell.org/ghcup/);
on Windows use GHCup under MSYS2). Two packages sit side by side: the
generated `azul` library and your executable. Keep them in *separate*
directories — cabal refuses two `.cabal` files in one directory.

```sh
# 1. the generated azul library package (-> ./azul-haskell/) and the example
#    executable package (-> ./): one bundle, unpacked in place
curl -LO https://azul.rs/ui/release/$VERSION/azul-haskell-$VERSION.tar.gz
tar xzf azul-haskell-$VERSION.tar.gz

# 2. the native library: pick your platform
curl -O https://azul.rs/ui/release/$VERSION/libazul.so     # linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib  # macOS
curl -O https://azul.rs/ui/release/$VERSION/azul.dll       # windows
```

The bundle contains `azul-haskell/azul.cabal`, `azul-haskell/src/Azul.hs`,
`Azul/Types.hs`, `Azul/Internal/FFI.hs`, the C shim layer the cabal file
compiles (`cbits/azul_shims.c` + `cbits/azul.h`), and `azul-example.cabal` +
`HelloWorld.hs` for the executable.

Add a two-line `cabal.project` next to `azul-example.cabal` so cabal
finds the in-tree `azul` package (it is not on Hackage):

```
packages: .
          ./azul-haskell
```

Then build and run (`--extra-lib-dirs` must be an *absolute* path —
ghc-pkg rejects relative ones during registration):

```sh
cabal build --extra-lib-dirs=$PWD

# linux
LD_LIBRARY_PATH=. cabal run hello-world
# macOS
DYLD_LIBRARY_PATH=. cabal run hello-world
# windows (azul.dll must be on PATH)
set PATH=%CD%;%PATH%
cabal run hello-world
```

The first `cabal build` compiles the whole generated binding (three
multi-megabyte modules plus the C shim) and takes a few minutes;
subsequent builds come from the cache.

## Simple "Counter" Example

This is the complete `HelloWorld.hs` (the same file the install step
downloads):

```haskell
module Main where

import Azul

newtype DataModel = DataModel { counter :: Int }

layout :: RefAny -> DataModel -> LayoutCallbackInfo -> IO Dom
layout dat model _ = do
  label <- domCreatePWithText (show (counter model)) >>= domWithCss "font-size: 32px;"
  button <- buttonCreate "Increase counter"
    >>= buttonWithButtonType ButtonType_Primary
    >>= buttonWithOnClick dat onClick
    >>= buttonDom
  domCreateBody >>= domWithChild label >>= domWithChild button

onClick :: RefAny -> CallbackInfo -> IO Update
onClick dat _ = refAnyUpdate dat (\m -> m { counter = counter m + 1 }) Update_RefreshDom

main :: IO ()
main = do
  dat <- refAnyCreate (DataModel 5)
  window <- windowCreateOptionsCreate layout
  appConfigCreate >>= appCreate dat >>= appRun window
```

### The model and `RefAny`

`refAnyCreate` stores any `Typeable` Haskell value in a table inside the
binding and hands libazul a `RefAny` that refers to it. Every clone
libazul makes of that `RefAny` — the one it passes to the layout callback,
the one the button keeps — refers to the same entry; the entry is
released when the last clone is dropped.

### The layout callback

`windowCreateOptionsCreate` accepts either the raw closure
`RefAny -> LayoutCallbackInfo -> IO Dom` or, as here, the model-typed
`RefAny -> DataModel -> LayoutCallbackInfo -> IO Dom`: the binding reads
the model out of the `RefAny` for you. The `RefAny` stays in scope because
attaching a child's callback needs it — `buttonWithOnClick dat onClick`
gives the button a clone of the app data.

Every generated method takes its receiver LAST, so the builder chains of
the other languages become `>>=` pipelines:
`domCreatePWithText "5" >>= domWithCss "font-size: 32px;"`. A method that
takes `self` by value (`with_child`, `dom`) *moves* its argument into
libazul; the wrapper you passed in is marked consumed and must not be
used again, which is exactly what a pipeline never does.

These functions live in `IO` because each one is a call into libazul on a
heap-owning value: a `Dom` is a tree of Rust vectors, and `with_child`
consumes both operands and returns the merged tree.

### The click handler

`onClick` has the same shape as in every other language — it receives the
`RefAny` and answers with an `Update` — and its body is one expression:
`refAnyUpdate dat f verdict` reads the model out of the `RefAny`, stores
`f model` back, and returns the verdict; `Update_RefreshDom` re-runs
`layout`. The downcast, the update and the upcast are the combinator's
business; a `RefAny` that holds another type is left untouched. When a
handler needs the model in `IO` — to do more than a pure update —
`refAnyGet :: RefAny -> IO (Maybe a)` and `refAnyModify :: RefAny -> (a -> a) -> IO ()`
are the fallback.

`buttonWithOnClick` also accepts a pure state transition
`DataModel -> CallbackInfo -> (DataModel, Update)` instead of the raw
closure: the binding applies it to the current model and stores the new
one. Both shapes are instances of the generated `ButtonOnClickCallbackHandler`
class, and every callback kind in api.json has the same pair. If the
`RefAny` does not hold a value of the handler's model type, the call is
logged on stderr and libazul's default (`Update_DoNothing`, an empty body)
applies.

### `main`

`appCreate` takes the app data and an `AppConfig`; `appRun` takes the
window options and the app. Both by-value arguments are moved into
libazul. `appRun` blocks until the window closes.

## Build and run

```sh
cabal build --extra-lib-dirs=$PWD
DYLD_LIBRARY_PATH=. cabal run hello-world
```

You should see the window pictured on the
[hello-world landing page](..md): the label renders "5"; each click runs
`onClick`, the binding stores the new model, and the framework re-runs
`layout` with it.

## Common errors

- **`cabal: Multiple cabal files found`** — `azul.cabal` and
  `azul-example.cabal` ended up in the same directory. Keep the
  generated library in its own subdirectory and point `cabal.project`
  at it.
- **C shim fails with `azul.h: No such file or directory`** — the
  header must sit *inside* `azul-haskell/cbits/` (the package compiles
  `cbits/azul_shims.c` with `include-dirs: cbits`).
- **Link error `cannot find -lazul`** — `--extra-lib-dirs` missing or
  relative. Pass an absolute path.
- **Runtime: `error while loading shared libraries` / `dyld: Library
  not loaded`** — set `LD_LIBRARY_PATH` (Linux) or `DYLD_LIBRARY_PATH`
  (macOS) to the directory containing the native library; on Windows
  put `azul.dll` on `PATH`.
- **`Ambiguous type variable` at a `refAnyUpdate` or `buttonWithOnClick`
  call** — a lambda gives the binding no model type to look up. Name the
  model type, as `onClick`'s record update and `layout`'s signature do.
- **`[azul] LayoutCallback callback raised: user error (... does not hold
  the model type ...)` on stderr and an empty window** — the `RefAny`
  given to `appCreate` was created from a value of another type than the
  layout's model.
- **A handler throws and the app keeps running** — exceptions never
  cross into libazul: the binding catches them, prints them to stderr
  and returns libazul's default for that callback kind.
- **The runtime aborts when a callback fires from another thread** —
  build executables with `-threaded` (the downloaded `azul-example.cabal`
  does); `ThreadCallback` and the `RefAny` releaser can run on threads
  libazul owns.
