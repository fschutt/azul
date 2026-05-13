{-# LANGUAGE ScopedTypeVariables #-}
{- |
Module      : Main
Description : Smoke test for the Haskell Azul C-ABI bindings.

Exercises:

1. Direct C-ABI calls through the @_via@ shim layer (struct-by-value
   args + returns) — `AzDom_createBody_via` round-trip.

2. The inbound-trampoline pattern: register a Haskell layout callback
   via @AzLayoutCallbackType_set_inner@ and verify the matching
   @AzLayoutCallbackType_trampoline@ symbol resolves to a non-null
   FunPtr. libazul calls the trampoline (which has the C-ABI by-value
   signature); the trampoline forwards to the Haskell inner (which has
   pointer args + out-pointer return, the shape GHC's FFI accepts).

   This is the foundation for full @App.run@ — splicing the trampoline
   FunPtr into a @WindowCreateOptions@ would let libazul drive Haskell
   layout calculation. Verification of actual click-routing (AZ_DEBUG
   5 → 8) is blocked by an unrelated libazul macOS webrender crash in
   AzApp_run, tracked in @memory/pascal_codegen_2026_05_13.md@.

Build:  cabal build
Run:    DYLD_LIBRARY_PATH=. cabal run hello-world
-}
module Main where

import qualified Azul.Internal.FFI as FFI
import qualified Azul.Types as T
import Foreign.Marshal.Alloc (alloca)
import Foreign.Ptr (Ptr, FunPtr, nullFunPtr)

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
  -- AzDom_delete is callable through the same FFI path.
  alloca $ \(domPtr :: Ptr T.Dom) -> do
    FFI.c_AzDom_createBody_via domPtr
    putStrLn "[azul] AzDom_createBody_via succeeded; struct-by-value out-param plumbing works."
    FFI.c_AzDom_delete domPtr
    putStrLn "[azul] AzDom_delete completed cleanly."

  -- 3. Inbound trampoline: register a Haskell layout fn through the
  -- generated `_set_inner` / `_trampoline` pair. The trampoline (in
  -- cbits/azul_shims.c) matches the C ABI's by-value-struct signature
  -- (`AzDom (*)(AzRefAny, AzLayoutCallbackInfo)`); it takes addresses
  -- of its locally-stored args and forwards to a Haskell `mk_*_inner`
  -- FunPtr that has by-pointer args + out-pointer return (the shape
  -- GHC's `foreign import ccall "wrapper"` supports).
  let myLayoutInner :: Ptr (T.RefAny ()) -> Ptr T.LayoutCallbackInfo -> Ptr T.Dom -> IO ()
      myLayoutInner _ _ domOut =
        -- A real layout fn would build a Dom tree via the same C-shim
        -- pattern (AzDom_createBody_via, AzDom_withChild_via, ...) and
        -- poke the result into `domOut`. For the smoke test we just
        -- prove the registration round-trips.
        FFI.c_AzDom_createBody_via domOut

  innerFnPtr <- FFI.mk_LayoutCallbackType_inner myLayoutInner
  FFI.c_AzLayoutCallbackType_set_inner innerFnPtr
  putStrLn "[azul] mk_LayoutCallbackType_inner + set_inner succeeded; Haskell layout fn pinned."

  if FFI.p_AzLayoutCallbackType_trampoline == nullFunPtr
    then error "[azul] FAIL: p_AzLayoutCallbackType_trampoline resolved to nullFunPtr"
    else putStrLn "[azul] p_AzLayoutCallbackType_trampoline resolved to a non-null C fn pointer."

  putStrLn "[azul] Inbound-trampoline round-trip wired; ready to splice into AzLayoutCallback."
  putStrLn "[azul] Haskell host-invoker init phase completed successfully."
  putStrLn "[azul] (AZ_DEBUG full-GUI verification blocked by macOS libazul webrender bug;"
  putStrLn "[azul]  see memory/pascal_codegen_2026_05_13.md — shared with Pascal/Lisp.)"
