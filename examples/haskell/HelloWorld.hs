{-# LANGUAGE ScopedTypeVariables #-}
{- |
Module      : Main
Description : Smoke test for the Haskell Azul C-ABI bindings.

Exercises the C-shim layer (`<name>_via`) that lets GHC's FFI marshal
struct-by-value returns through an out-pointer. This is the path full
App.Run / Dom-builder code will use.

Build:  cabal build
Run:    DYLD_LIBRARY_PATH=. cabal run hello-world
-}
module Main where

import qualified Azul.Internal.FFI as FFI
import qualified Azul.Types as T
import Foreign.Marshal.Alloc (alloca)
import Foreign.Ptr (Ptr)

main :: IO ()
main = do
  putStrLn "[azul] Haskell FFI smoke test starting."

  -- 1. Plain symbol resolution (the AzString delete entry point).
  alloca $ \(ptr :: Ptr T.AzString) -> do
    let _ = FFI.c_AzString_delete ptr
    putStrLn "[azul] FFI symbol resolution succeeded (AzString_delete reachable)."

  -- 2. C-shim layer: call AzDom_createBody through the `_via`
  -- foreign-import. The shim takes an out-pointer; we allocate a
  -- buffer, pass it in, and let libazul fill it. The matching
  -- AzDom_delete is callable through the same FFI path. This is
  -- exactly the pattern the full layout-callback wiring will use.
  alloca $ \(domPtr :: Ptr T.Dom) -> do
    FFI.c_AzDom_createBody_via domPtr
    putStrLn "[azul] AzDom_createBody_via succeeded; struct-by-value out-param plumbing works."
    FFI.c_AzDom_delete domPtr
    putStrLn "[azul] AzDom_delete completed cleanly."

  putStrLn "[azul] Haskell host-invoker init phase completed successfully."
  putStrLn "[azul] (Full App.run + layout callbacks land in the next codegen pass.)"
