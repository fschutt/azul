{-# LANGUAGE ScopedTypeVariables #-}
-- cabal run hello-world

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
