{-# LANGUAGE ScopedTypeVariables #-}
{- |
Module      : Main
Description : Smoke test for the Haskell Azul C-ABI bindings.

Phase H.1 lands the `registerLayoutCallbackTypeCallback` helper that
hides the inbound-trampoline triplet. The user-facing layout fn now
reads natively:

>>> myLayout :: Ptr (RefAny ()) -> Ptr LayoutCallbackInfo -> IO Dom
>>> myLayout _ _ = alloca $ \buf -> do
>>>     c_AzDom_createBody_via buf
>>>     peek buf

AZ_DEBUG full-GUI verification is still blocked by the libazul macOS
webrender crash (same as Pascal — see memory/pascal_codegen_2026_05_13.md).

Build:  cabal build
Run:    DYLD_LIBRARY_PATH=. cabal run hello-world
-}
module Main where

import Azul.Internal.FFI
import qualified Azul.Types as T
import Foreign.Marshal.Alloc (alloca)
import Foreign.Ptr (Ptr, FunPtr, nullFunPtr)
import Foreign.Storable (peek)

main :: IO ()
main = do
  putStrLn "[azul] Haskell FFI smoke test starting."

  -- 1. Plain symbol resolution.
  alloca $ \(ptr :: Ptr T.AzString) -> do
    let _ = c_AzString_delete ptr
    putStrLn "[azul] FFI symbol resolution succeeded (AzString_delete reachable)."

  -- 2. Outbound `_via` shim: struct-by-value return through out-ptr.
  alloca $ \(domPtr :: Ptr T.Dom) -> do
    c_AzDom_createBody_via domPtr
    putStrLn "[azul] AzDom_createBody_via succeeded; struct-by-value out-param plumbing works."
    c_AzDom_delete domPtr
    putStrLn "[azul] AzDom_delete completed cleanly."

  -- 3. Phase H.1: register a layout callback through the smart helper.
  -- The helper hides mk_inner + set_inner + trampoline; user code just
  -- writes a natural `data -> info -> IO Dom` function.
  let myLayout :: Ptr (T.RefAny ()) -> Ptr T.LayoutCallbackInfo -> IO T.Dom
      myLayout _ _ = alloca $ \buf -> do
        c_AzDom_createBody_via buf
        peek buf

  cbPtr <- registerLayoutCallbackTypeCallback myLayout
  putStrLn "[azul] registerLayoutCallbackTypeCallback returned a non-null FunPtr."

  if cbPtr == nullFunPtr
    then error "[azul] FAIL: registerLayoutCallbackTypeCallback returned nullFunPtr"
    else putStrLn "[azul] Phase H.1 helper smoke succeeded; ready to splice into AzLayoutCallback."

  putStrLn "[azul] Haskell host-invoker init phase completed successfully."
  putStrLn "[azul] (AZ_DEBUG full-GUI verification blocked by macOS libazul webrender bug;"
  putStrLn "[azul]  see memory/pascal_codegen_2026_05_13.md — shared with Pascal/Lisp.)"
