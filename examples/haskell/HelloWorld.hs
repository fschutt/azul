{-# LANGUAGE ScopedTypeVariables #-}
module Main where

import Azul.Internal.FFI
import qualified Azul.Types as T
import Data.IORef
import Foreign.C.String (withCAStringLen)
import Foreign.C.Types (CSize)
import Foreign.Marshal.Alloc (alloca, allocaBytes, mallocBytes)
import Foreign.Marshal.Utils (fillBytes)
import Foreign.Ptr (Ptr, FunPtr, castPtr)
import Foreign.Storable (poke, sizeOf)

szRefAny, szString, szDom, szButton, szWco, szAppConfig, szApp, szOnClickCb :: Int
szRefAny    = 32
szString    = sizeOf (undefined :: T.AzString)
szDom       = sizeOf (undefined :: T.Dom)
szButton    = sizeOf (undefined :: T.Button)
szWco       = sizeOf (undefined :: T.WindowCreateOptions)
szAppConfig = sizeOf (undefined :: T.AppConfig)
szApp       = sizeOf (undefined :: T.App)
szOnClickCb = sizeOf (undefined :: T.ButtonOnClickCallback)

mkAzString :: String -> Ptr T.AzString -> IO ()
mkAzString s out =
  withCAStringLen s $ \(p, len) ->
    c_AzString_copyFromBytes_via (castPtr p) 0 (fromIntegral len :: CSize) out

mkPlaceholderRefAny :: FunPtr () -> Ptr (T.RefAny ()) -> IO ()
mkPlaceholderRefAny dtorTramp out =
  allocaBytes 16 $ \(gvp :: Ptr T.GlVoidPtrConst) -> do
    fillBytes gvp 0 16
    allocaBytes szString $ \typeName -> do
      mkAzString "HsCounterModel" typeName
      alloca $ \(dtorCell :: Ptr (FunPtr ())) -> do
        poke dtorCell dtorTramp
        c_AzRefAny_newC_via gvp 0 1 0xBA5EBA11 typeName (castPtr dtorCell) 0 0 out

buildLayout :: IORef Int
            -> Ptr (T.RefAny ())
            -> Ptr T.ButtonOnClickCallback
            -> Ptr (T.RefAny ()) -> Ptr T.LayoutCallbackInfo -> Ptr T.Dom -> IO ()
buildLayout counter master clickCb _data _info outPtr = do
  n <- readIORef counter
  c_AzDom_createBody_via outPtr

  allocaBytes szDom $ \labelDom -> do
    allocaBytes szString $ \text -> do
      mkAzString (show n) text
      c_AzDom_createPWithText_via text labelDom
    allocaBytes szString $ \css -> do
      mkAzString "font-size: 32px;" css
      c_AzDom_setCss_via labelDom css
    c_AzDom_addChild_via outPtr labelDom

  allocaBytes szButton $ \btn -> do
    allocaBytes szString $ \label -> do
      mkAzString "Increase counter" label
      c_AzButton_create_via label btn
    alloca $ \(btnType :: Ptr T.ButtonType) -> do
      poke btnType T.ButtonType_Primary
      c_AzButton_setButtonType_via btn btnType
    allocaBytes szRefAny $ \dataClone -> do
      c_AzRefAny_clone_via master dataClone
      c_AzButton_setOnClick_via btn dataClone clickCb
    allocaBytes szDom $ \btnDom -> do
      c_AzButton_dom_via btn btnDom
      c_AzDom_addChild_via outPtr btnDom

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
          c_AzApp_create_via appData cfg app
          c_AzApp_run_via app wco

  putStrLn "[azul] App exited cleanly."
