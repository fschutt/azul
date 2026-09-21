---
slug: hello-world/haskell
title: Hello World [Haskell]
language: en
canonical_slug: hello-world/haskell
audience: external
maturity: mature
guide_order: 27
topic_only: false
prerequisites: [hello-world]
tracked_files:
  - api.json
  - examples/haskell/HelloWorld.hs
  - examples/haskell/azul-example.cabal
last_generated_rev: 2660b0c45c9ea401ad6777a203f468755167e62e
generated_at: 2026-09-16T00:00:00Z
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

To use `libazul` from Haskell, you need the native prebuilt library and the generated `azul`
cabal package, which calls the C API through GHC's foreign function interface. The bindings 
convert between your data model and the `RefAny` which the framework stores, and passes structs 
through small C shims that cabal compiles with it. The package is tested with GHC 9.6 and 9.14.

## Installation

You need GHC and cabal, for example from [GHCup](https://www.haskell.org/ghcup/) or with
`brew install ghc cabal-install`. On Windows, use the MSYS2 shell that GHCup installs.

The release bundle contains the example (`HelloWorld.hs`, `azul-example.cabal`,
`cabal.project`) and the generated `azul` package in `azul-haskell/`.

macOS:

```sh
mkdir hello-world && cd hello-world

# macOS
curl -O https://azul.rs/ui/release/$VERSION/libazul.dylib
# Linux
curl -O https://azul.rs/ui/release/$VERSION/libazul.so
# Windows 
curl -O https://azul.rs/ui/release/$VERSION/azul.dll

curl -LO https://azul.rs/ui/release/$VERSION/azul-haskell-$VERSION.tar.gz
tar xzf azul-haskell-$VERSION.tar.gz
cabal run --extra-lib-dirs=$PWD hello-world
```

`--extra-lib-dirs` tells the linker where the native library is. 
It must be an absolute path, and it must stay the same between runs: cabal
treats it as part of the build configuration and rebuilds the whole package 
when it changes.

The first run compiles the generated package (about a thousand small modules 
and the C shims) in parallel, which takes about 2-3 minutes on an 8-core machine. 
Later runs only compile your own code.

### Building from source

Only needed if you want to track `master` or patch the library locally:

```sh
# git clone https://github.com/fschutt/azul
# cd myfolder/azul
# generate the bindings from api.json (required)
cargo run -p azul-doc --release -- codegen all
# build the actual DLL with the now-generated .rs C-API bindings
cargo build -p azul-dll --release --features build-dll
```

Notice the required `--features build-dll`. The DLL lands in
`target/release/libazul.{so,dylib}` (or `azul.dll`), the cabal package 
in `target/codegen/haskell/`. The package also needs the C header in 
its `cbits/` directory:

```sh
cp -R target/codegen/haskell my_app/azul-haskell
cp target/codegen/azul.h my_app/azul-haskell/cbits/
cp examples/haskell/{HelloWorld.hs,azul-example.cabal,cabal.project} my_app/
cp target/release/libazul.dylib my_app/
```

## Simple "Counter" Example

```haskell
module Main where

import Azul
import qualified Azul.App as App
import qualified Azul.AppConfig as AppConfig
import qualified Azul.Button as Button
import qualified Azul.Dom as Dom
import qualified Azul.WindowCreateOptions as WindowCreateOptions

newtype DataModel = DataModel { counter :: Int }

onClick :: DataModel -> CallbackInfo -> (DataModel, Update)
onClick model _ = (model { counter = counter model + 1 }, Update_RefreshDom)

layout :: DataModel -> LayoutCallbackInfo -> IO Dom
layout model _ = do
  label <- Dom.createPWithText (show (counter model)) >>= Dom.withCss "font-size: 32px; margin: 0;"
  button <- Button.create "Increase counter"
    >>= Button.withButtonType ButtonType_Primary
    >>= Button.onClick onClick
    >>= Button.dom
  Dom.createBody >>= Dom.withChild label >>= Dom.withChild button

main :: IO ()
main = do
  window <- WindowCreateOptions.create layout
  AppConfig.create >>= App.create (DataModel 5) >>= App.run window
```

## Notes

Here, `import Azul` brings the types into scope. Each class's constructors and methods 
live in its own module, imported qualified: `Button.create`, `Dom.withChild`, like 
`Map.insert` from `Data.Map`.

`onClick` is a pure function: it gets the current model and returns the new model together
with an `Update`. The package stores the new model; `Update_RefreshDom` runs `layout` again.
A handler that needs `IO` has the type `DataModel -> CallbackInfo -> IO (DataModel, Update)`.

`Button.onClick` attaches the handler to the model of the running callback, here the model
`layout` was called with. `App.create` takes the initial model, any Haskell value.

If a handler expects another model type, the package logs
`azul: ButtonOnClickCallback expected a model of type OtherModel, got DataModel` and doesn't
call it. An exception inside a callback is caught and logged the same way and the app keeps
running.

Every function takes its receiver last, so builder calls chain with `>>=`. Arguments passed
by value are moved into the library: after `Dom.withChild label`, `label` cannot be used again.
Using it anyway raises an `AzulError` instead of crashing.

## Build and run

From the directory containing `cabal.project` and the native library:

```sh
export LD_LIBRARY_PATH=. # Linux only

cabal run --extra-lib-dirs=$PWD hello-world
```

You should see the window pictured on the [hello-world landing page](../hello-world.md).
Click the button: the counter should increment, the layout callback then re-runs, and the new value renders.

1. `App.run` opens a native window and runs `layout` once with your model.
2. The returned DOM is styled, laid out, and rendered.
3. The framework then continuously queries whether anything matches the event filter set up
   in the DOM. On click, the framework runs `onClick` with the current model, stores the model
   it returns, observes the refresh return, and re-invokes the layout callback.
4. The framework determines the diff between the previous frame's DOM and the current one,
   and only re-updates and re-paints the counter, not the entire window.

Congratulations! Once you've got the hello-world example running, you've already mastered 80%
of the framework. As you might have guessed, more complex UI and styling are only composing
more Dom objects together and working with the various event filters.

You can now start reading about the [architecture patterns](../architecture.md) or
explore what [methods the `Dom` has to offer](../dom.md).

See you in the next tutorial!
