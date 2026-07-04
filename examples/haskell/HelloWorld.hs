{-# LANGUAGE ScopedTypeVariables #-}
-- cabal run hello-world
--
-- Full-GUI counter hello-world for the Azul Haskell bindings.
--
-- DOM shape (matches tests/e2e/hello_world_counter.json):
--   body
--   ├── div { font-size: 32px }
--   │   └── text("5")            -- the counter, increments on click
--   └── Button "Increase counter"
--
-- Callback mechanism: the Haskell binding uses inbound TRAMPOLINES.
-- cbits/azul_shims.c holds one static C function per callback typedef
-- (Az<X>Type_trampoline) plus a global "inner" slot (Az<X>Type_set_inner).
-- We create a Haskell FunPtr with the codegen's `mk_<X>_inner` wrapper,
-- store it in the slot, and hand libazul the trampoline's address.
-- Because the inner is a Haskell *closure*, application state lives in a
-- plain IORef captured by the closures — no downcast through RefAny needed
-- (the RefAny we pass around is a zero-sized placeholder that satisfies
-- libazul's refcounting).
--
-- NOTE on style: the DOM is built exclusively with the raw `c_Az*_via`
-- out-pointer primitives (writing straight into C-provided buffers).
-- The generated struct Storables for DOM-sized aggregates contain
-- tagged-union placeholders whose peek/poke intentionally `error` out, so
-- value-level round-trips through `T.Dom` must be avoided.

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

-- | Write an owned AzString (copied from a Haskell String) into @out@.
-- ASCII-only content in this example, so the Latin-1 marshalling is
-- valid UTF-8.
mkAzString :: String -> Ptr T.AzString -> IO ()
mkAzString s out =
  withCAStringLen s $ \(p, len) ->
    c_AzString_copyFromBytes_via (castPtr p) 0 (fromIntegral len :: CSize) out

-- | Build a zero-sized placeholder RefAny. State does NOT live here — it
-- lives in the IORef captured by the callback closures. libazul still
-- clones/drops this, so it must be a real refcounted RefAny.
mkPlaceholderRefAny :: FunPtr () -> Ptr (T.RefAny ()) -> IO ()
mkPlaceholderRefAny dtorTramp out =
  allocaBytes 16 $ \(gvp :: Ptr T.GlVoidPtrConst) -> do
    fillBytes gvp 0 16                    -- { ptr = NULL, run_destructor = false }
    allocaBytes szString $ \typeName -> do
      mkAzString "HsCounterModel" typeName
      alloca $ \(dtorCell :: Ptr (FunPtr ())) -> do
        poke dtorCell dtorTramp
        c_AzRefAny_newC_via gvp 0 1 0xBA5EBA11 typeName (castPtr dtorCell) 0 0 out

-- | The layout function: rebuilds the whole DOM from the counter value.
-- Writes the resulting AzDom directly into the trampoline's out-pointer.
buildLayout :: IORef Int
            -> Ptr (T.RefAny ())            -- master placeholder RefAny (cloned per button)
            -> Ptr T.ButtonOnClickCallback  -- prepared { cb = trampoline, callable = None }
            -> Ptr (T.RefAny ()) -> Ptr T.LayoutCallbackInfo -> Ptr T.Dom -> IO ()
buildLayout counter master clickCb _data _info outPtr = do
  n <- readIORef counter
  c_AzDom_createBody_via outPtr

  -- div { font-size: 32px } > text(show n)
  allocaBytes szDom $ \divBuf -> do
    c_AzDom_createDiv_via divBuf
    allocaBytes szString $ \css -> do
      mkAzString "font-size: 32px;" css
      c_AzDom_setCss_via divBuf css       -- consumes css
    allocaBytes szDom $ \txt ->
      allocaBytes szString $ \label -> do
        mkAzString (show n) label
        c_AzDom_createText_via label txt  -- consumes label
        c_AzDom_addChild_via divBuf txt   -- consumes txt
    c_AzDom_addChild_via outPtr divBuf    -- consumes divBuf

  -- Button "Increase counter" (typed ButtonOnClick callback)
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

  -- 1. No-op RefAny destructor (payload is zero-sized; nothing to free).
  dtorInner <- mk_RefAnyDestructorType_inner (\_ -> pure ())
  c_AzRefAnyDestructorType_set_inner dtorInner

  -- 2. Master placeholder RefAny (lives for the program's lifetime).
  master <- mallocBytes szRefAny :: IO (Ptr (T.RefAny ()))
  mkPlaceholderRefAny p_AzRefAnyDestructorType_trampoline master

  -- 3. Typed button on-click: increment the IORef, request a re-layout.
  clickInner <- mk_ButtonOnClickCallbackType_inner $ \_data _info out -> do
    modifyIORef' counter (+ 1)
    poke out T.Update_RefreshDom
  c_AzButtonOnClickCallbackType_set_inner clickInner
  -- AzButtonOnClickCallback value = { cb = trampoline, callable = None(0) }.
  clickCb <- mallocBytes szOnClickCb :: IO (Ptr T.ButtonOnClickCallback)
  fillBytes clickCb 0 szOnClickCb
  poke (castPtr clickCb :: Ptr (FunPtr ())) p_AzButtonOnClickCallbackType_trampoline

  -- 4. Layout callback: builds the DOM into the trampoline's out-pointer.
  layoutInner <- mk_LayoutCallbackType_inner (buildLayout counter master clickCb)
  c_AzLayoutCallbackType_set_inner layoutInner

  -- 5. WindowCreateOptions(layout_callback) + AppConfig + App, then run.
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
