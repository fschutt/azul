module Main where

import Azul
import qualified Azul.App as App
import qualified Azul.AppConfig as AppConfig
import qualified Azul.Button as Button
import qualified Azul.Dom as Dom
import qualified Azul.WindowCreateOptions as WindowCreateOptions

newtype DataModel = DataModel { counter :: Int }

onClick :: DataModel -> CallbackInfo -> (DataModel, Update)
onClick model _ = (model { counter = counter model + 1 }, Update_RefreshDom)

layout :: DataModel -> LayoutCallbackInfo -> IO Dom
layout model _ = do
  label <- Dom.createPWithText (show (counter model)) >>= Dom.withCss "font-size: 32px; margin: 0;"
  button <- Button.create "Increase counter"
    >>= Button.withButtonType ButtonType_Primary
    >>= Button.onClick onClick
    >>= Button.dom
  Dom.createBody >>= Dom.withChild label >>= Dom.withChild button

main :: IO ()
main = do
  window <- WindowCreateOptions.create layout
  AppConfig.create >>= App.create (DataModel 5) >>= App.run window
