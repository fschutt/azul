{-# LANGUAGE ScopedTypeVariables #-}
-- Memory test for the azul Haskell binding. See tests/memtest/README.md.
--
-- The harness (scripts/run_memtest.sh) measures peak RSS across a small and a
-- large AZ_MEMTEST_N (RSS that scales with N is a LEAK) and fails on any crash.
-- This file only exercises the create/consume/DROP paths in a loop and exits 0.
-- No event loop (AzApp_run needs a display and hangs headless).
--
-- Uses the raw out-pointer `c_Az*_via` primitives, like examples/haskell.

module Main where

import Azul.Internal.FFI
import qualified Azul.Types as T
import Foreign.C.String (withCAStringLen)
import Foreign.C.Types (CSize)
import Foreign.Marshal.Alloc (alloca, allocaBytes, mallocBytes)
import Foreign.Marshal.Utils (fillBytes)
import Foreign.Ptr (Ptr, FunPtr, castPtr)
import Foreign.Storable (poke)
import System.Environment (lookupEnv)
import Text.Read (readMaybe)

-- C ABI sizes (generous; checked against sizeof() of the shipped azul.h).
szRefAny, szString, szAppConfig, szApp :: Int
szRefAny    = 32    -- sizeof(AzRefAny)    = 24
szString    = 48    -- sizeof(AzString)    = 40
szAppConfig = 2048  -- sizeof(AzAppConfig) = 1648
szApp       = 64    -- sizeof(AzApp)       = 16

mkAzString :: String -> Ptr T.AzString -> IO ()
mkAzString s out =
  withCAStringLen s $ \(p, len) ->
    c_AzString_copyFromBytes_via (castPtr p) 0 (fromIntegral len :: CSize) out

-- Zero-sized placeholder RefAny (real refcounted handle, no payload to free).
mkPlaceholderRefAny :: FunPtr () -> Ptr (T.RefAny ()) -> IO ()
mkPlaceholderRefAny dtorTramp out =
  allocaBytes 16 $ \(gvp :: Ptr T.GlVoidPtrConst) -> do
    fillBytes gvp 0 16                    -- { ptr = NULL, run_destructor = false }
    allocaBytes szString $ \typeName -> do
      mkAzString "HsCounterModel" typeName
      alloca $ \(dtorCell :: Ptr (FunPtr ())) -> do
        poke dtorCell dtorTramp
        c_AzRefAny_newC_via gvp 0 1 0xBA5EBA11 typeName (castPtr dtorCell) 0 0 out

main :: IO ()
main = do
  n <- maybe 200000 id . (>>= readMaybe) <$> lookupEnv "AZ_MEMTEST_N"

  -- No-op RefAny destructor + master placeholder handle.
  dtorInner <- mk_RefAnyDestructorType_inner (\_ -> pure ())
  c_AzRefAnyDestructorType_set_inner dtorInner
  master <- mallocBytes szRefAny :: IO (Ptr (T.RefAny ()))
  mkPlaceholderRefAny p_AzRefAnyDestructorType_trampoline master

  -- 1. The consume-by-value DROP path: AzApp_create consumes appData + the
  --    AppConfig (nested SystemStyle); AzApp_delete drops the App once.
  allocaBytes szAppConfig $ \cfg -> do
    c_AzAppConfig_create_via cfg
    allocaBytes szRefAny $ \appData -> do
      c_AzRefAny_clone_via master appData
      allocaBytes szApp $ \app -> do
        c_AzApp_create_via appData cfg app
        c_AzApp_delete app

  -- 2. Leak loop: create/destroy a droppable AppConfig N times.
  let loop :: Int -> IO ()
      loop 0 = pure ()
      loop k = do
        allocaBytes szAppConfig $ \cfg -> do
          c_AzAppConfig_create_via cfg
          c_AzAppConfig_delete cfg
        loop (k - 1)
  loop n

  putStrLn ("memtest haskell OK (N=" ++ show n ++ ")")
