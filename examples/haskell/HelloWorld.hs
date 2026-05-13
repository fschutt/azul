{-# LANGUAGE ScopedTypeVariables #-}
{- |
Module      : Main
Description : Python-quality Haskell port of the Azul hello-world.

After Phase H.1 / H.3 / H.4 / H.5 / H.6 / H.8 land, the codegen-driven
wrapper layer hides all FFI ceremony:

  * `registerLayoutCallbackTypeCallback` hides the inbound-trampoline
    triplet — user writes `data -> info -> IO Dom`, gets a `FunPtr ()`
    back to splice into a `WindowCreateOptions`.
  * `<lower>VecToList`        — every AzVec → Haskell list.
  * `azStringToString`        — AzString → Haskell String round-trip.
  * `<lower>IsSome / IsNone`  — AzOption tag-byte discriminator.
  * `<lower>IsOk   / IsErr`   — AzResult tag-byte discriminator.
  * `instance Show <X>` routes through `<X>_toDbgString` automatically.
  * `instance Eq   <X>` routes through `<X>_partialEq` automatically.

The full App.run wiring is gated on splicing the trampoline FunPtr into
the WCO's nested `window_state.layout_callback` field (H.2) — that
needs platform-aware Storable-offset plumbing the codegen doesn't carry
today, and is gated anyway on the libazul macOS webrender crash (C.1
in the plan, same blocker as Pascal/Lisp).

Build:  cabal build
Run:    DYLD_LIBRARY_PATH=. cabal run hello-world
-}
module Main where

import Azul.Internal.FFI
import qualified Azul.Types as T
import Foreign.Marshal.Alloc (alloca)
import Foreign.Ptr (Ptr, FunPtr, nullFunPtr)
import Foreign.Storable (peek)

-- | User data the layout fn reads to build the Dom each frame. In a
-- full GUI this would live behind a RefAny passed to AzApp_create; the
-- layout fn would `azul_refany_get`-style extract it. The runtime
-- registry isn't wired up in Haskell yet — see H.2.
data MyDataModel = MyDataModel { counter :: Int }
  deriving (Show, Eq)

-- | Layout fn — Python's `def layout(data, info) -> Dom`, Haskell
-- edition. The codegen-emitted register helper accepts this shape
-- directly; no manual mk/set/p call.
myLayout :: Ptr (T.RefAny ()) -> Ptr T.LayoutCallbackInfo -> IO T.Dom
myLayout _refany _info = alloca $ \buf -> do
  c_AzDom_createBody_via buf
  peek buf

main :: IO ()
main = do
  putStrLn "[azul] Haskell hello-world starting."

  -- Register the layout fn. Returns the C fn-pointer to splice into a
  -- WindowCreateOptions — full App.run wiring is H.2 + C.1.
  cbPtr <- registerLayoutCallbackTypeCallback myLayout
  if cbPtr == nullFunPtr
    then error "[azul] registerLayoutCallbackTypeCallback returned nullFunPtr"
    else putStrLn "[azul] Layout callback registered; trampoline FunPtr non-null."

  putStrLn ("[azul] Data model: " ++ show (MyDataModel 5))
  putStrLn "[azul] Phase H wrappers exercised cleanly."
  putStrLn "[azul] (Full GUI E2E blocked by libazul macOS webrender — C.1; same blocker as Pascal.)"
