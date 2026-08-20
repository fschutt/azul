{-# LANGUAGE ScopedTypeVariables #-}
-- GOTCHA: build the DOM only with the raw `c_Az*_via` out-pointer primitives.
-- The Storables for DOM-sized aggregates hold tagged-union placeholders whose
-- peek/poke intentionally `error` out, so never round-trip through `T.Dom`.

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

-- ASCII-only here, so the Latin-1 marshalling is valid UTF-8.
mkAzString :: String -> Ptr T.AzString -> IO ()
mkAzString s out =
  withCAStringLen s $ \(p, len) ->
    c_AzString_copyFromBytes_via (castPtr p) 0 (fromIntegral len :: CSize) out

-- libazul clones/drops this placeholder, so it must be a real refcounted RefAny.
mkPlaceholderRefAny :: FunPtr () -> Ptr (T.RefAny ()) -> IO ()
mkPlaceholderRefAny dtorTramp out =
  allocaBytes 16 $ \(gvp :: Ptr T.GlVoidPtrConst) -> do
    fillBytes gvp 0 16                    -- { ptr = NULL, run_destructor = false }
    allocaBytes szString $ \typeName -> do
      mkAzString "HsCounterModel" typeName
      alloca $ \(dtorCell :: Ptr (FunPtr ())) -> do
        poke dtorCell dtorTramp
        c_AzRefAny_newC_via gvp 0 1 0xBA5EBA11 typeName (castPtr dtorCell) 0 0 out

buildLayout :: IORef Int
            -> Ptr (T.RefAny ())            -- master placeholder RefAny (cloned per button)
            -> Ptr T.ButtonOnClickCallback  -- prepared { cb = trampoline, callable = None }
            -> Ptr (T.RefAny ()) -> Ptr T.LayoutCallbackInfo -> Ptr T.Dom -> IO ()
buildLayout counter master clickCb _data _info outPtr = do
  n <- readIORef counter
  c_AzDom_createBody_via outPtr

  allocaBytes szDom $ \divBuf -> do
    c_AzDom_createDiv_via divBuf
    allocaBytes szString $ \css -> do
      mkAzString "font-size: 32px;" css
      c_AzDom_setCss_via divBuf css       -- consumes css
    allocaBytes szDom $ \txt ->
      allocaBytes szString $ \label -> do
        mkAzString (show n) label
        c_AzDom_createTextDoNotUseWithoutBlockLevelWrapper_via label txt  -- consumes label
        c_AzDom_addChild_via divBuf txt   -- consumes txt
    c_AzDom_addChild_via outPtr divBuf    -- consumes divBuf

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

  dtorInner <- mk_RefAnyDestructorType_inner (\_ -> pure ())
  c_AzRefAnyDestructorType_set_inner dtorInner

  master <- mallocBytes szRefAny :: IO (Ptr (T.RefAny ()))
  mkPlaceholderRefAny p_AzRefAnyDestructorType_trampoline master

  clickInner <- mk_ButtonOnClickCallbackType_inner $ \_data _info out -> do
    modifyIORef' counter (+ 1)
    poke out T.Update_RefreshDom
  c_AzButtonOnClickCallbackType_set_inner clickInner
  -- AzButtonOnClickCallback value = { cb = trampoline, callable = None(0) }.
  clickCb <- mallocBytes szOnClickCb :: IO (Ptr T.ButtonOnClickCallback)
  fillBytes clickCb 0 szOnClickCb
  poke (castPtr clickCb :: Ptr (FunPtr ())) p_AzButtonOnClickCallbackType_trampoline

  layoutInner <- mk_LayoutCallbackType_inner (buildLayout counter master clickCb)
  c_AzLayoutCallbackType_set_inner layoutInner

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
