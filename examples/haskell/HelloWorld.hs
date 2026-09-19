module Main where

import Azul

newtype DataModel = DataModel { counter :: Int }

onClick :: DataModel -> CallbackInfo -> (DataModel, Update)
onClick model _ = (model { counter = counter model + 1 }, Update_RefreshDom)

layout :: DataModel -> LayoutCallbackInfo -> IO Dom
layout model _ = do
  label <- domCreatePWithText (show (counter model)) >>= domWithCss "font-size: 32px; margin: 0;"
  button <- buttonCreate "Increase counter"
    >>= buttonWithButtonType ButtonType_Primary
    >>= buttonOnClick onClick
    >>= buttonDom
  domCreateBody >>= domWithChild label >>= domWithChild button

main :: IO ()
main = do
  window <- windowCreateOptionsCreate layout
  appConfigCreate >>= appCreate (DataModel 5) >>= appRun window
