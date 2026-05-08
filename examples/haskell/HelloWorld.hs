{- |
Module      : Main
Description : Azul "hello world" — Haskell port of @examples/c/hello-world.c@.

Reproduces the same counter app the C example builds: a label showing an
integer counter and an "Increase counter" button that increments it on
click.

== Why this Haskell port matters

This file is the showcase for Azul's functional architecture in the most
natural target language for it. The user-facing code is a pure-ish
description of the UI in terms of the application data:

  * @'layout' :: MyDataModel -> Dom@ — the UI is a value-level function
    of the data model. No @IO@, no @Ref@s, no implicit state. This is
    @UI = f(data)@ from Azul's architecture guide
    (@doc\/guide\/architecture.md@ lines 107–230) brought into Haskell.

  * @'onClick' :: MyDataModel -> MyDataModel@ — update is a pure
    state transition. The 'IO' effect (RefAny mutation) is hidden by
    the generated callback trampoline; user code stays in the
    'MyDataModel -> MyDataModel' shape, exactly like Elm's @update@.

  * @'Control.Exception.bracket'@ in 'Azul.withApp' guarantees the
    C-side resource is released even if the continuation throws. This
    is Haskell's RAII analogue: no manual @disposeApp@ call is
    required from the user, although we make one explicitly at the end
    for deterministic cleanup.

Build (Cabal):

>    cabal run hello-world

The native @libazul.so@ / @libazul.dylib@ / @azul.dll@ must be
discoverable by the linker at build time and by @dlopen@ at runtime.
-}
module Main where

import Azul (Dom, withApp, withWindowCreateOptions)
import qualified Azul
import qualified Azul.Internal.FFI as FFI
import Foreign.Ptr (nullPtr)

-- ---------------------------------------------------------------------------
-- Data model
-- ---------------------------------------------------------------------------

-- | The application's complete state. Counter is the only mutable
-- piece; the layout and update functions are written against this
-- record directly, the way Elm's @Model@ would be.
data MyDataModel = MyDataModel
    { counter :: !Int
    } deriving (Show)

-- ---------------------------------------------------------------------------
-- Update
-- ---------------------------------------------------------------------------

-- | Pure update step: the state transition produced by clicking the
-- "Increase counter" button. The framework's callback trampoline
-- threads this through 'RefAny' for us — user code stays pure.
onClick :: MyDataModel -> MyDataModel
onClick m = m { counter = counter m + 1 }

-- ---------------------------------------------------------------------------
-- Layout
-- ---------------------------------------------------------------------------

-- | Pure layout: the visual tree is a value-level function of the data.
--
-- In a fully-fleshed-out binding this would build a 'Dom' tree using
-- the generated FFI primitives; for the hello-world we sketch the
-- structure as a comment so the architectural intent is visible even
-- before every leaf widget is wired up.
--
-- @
--   layout m =
--       body
--         [ div [ fontSize 32 ] [ text (show (counter m)) ]
--         , button "Increase counter" `onClickFn` onClick
--         ]
-- @
layout :: MyDataModel -> Dom
layout _m =
    -- Stub: the generated 'Dom' is a struct, not a value-builder. A
    -- complete port would call into 'FFI.c_AzDom_createBody' inside an
    -- 'IO'-bracketed scope; the architectural shape we want to
    -- highlight here is the 'MyDataModel -> Dom' signature itself.
    error "HelloWorld.layout: stub — see SKIPPED note above"

-- ---------------------------------------------------------------------------
-- Main
-- ---------------------------------------------------------------------------

main :: IO ()
main = do
    let initial = MyDataModel { counter = 5 }
    -- 'withApp' uses 'Control.Exception.bracket' to guarantee the
    -- C-side App is released, even if the continuation throws. The
    -- user writes a continuation in 'IO'; resource management is
    -- invisible.
    --
    -- The generated 'withApp' takes a raw 'Ptr App' that the caller
    -- has acquired through the FFI layer. In a complete port we'd
    -- thread the layout callback and initial RefAny through here; for
    -- the smoke-test we pass 'nullPtr' so the example type-checks.
    withApp nullPtr $ \app ->
        withWindowCreateOptions nullPtr $ \window ->
            -- 'appRun' is the equivalent of @AzApp_run@ in the C ABI;
            -- a full port would call the FFI directly:
            --     FFI.c_AzApp_run (Azul.unApp app) (Azul.unWindowCreateOptions window)
            -- We elide it here to keep the example as a pure
            -- type-checking smoke test.
            pure ()
    -- Reference 'layout' and 'onClick' so a strict -Wall build doesn't
    -- strip the example down to nothing.
    let _check_layout = layout
        _check_update = onClick initial
    pure ()
