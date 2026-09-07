module Main where

import Azul

newtype DataModel = DataModel { counter :: Int }

layout :: RefAny -> DataModel -> LayoutCallbackInfo -> IO Dom
layout dat model _ = do
  label <- domCreatePWithText (show (counter model)) >>= domWithCss "font-size: 32px;"
  button <- buttonCreate "Increase counter"
    >>= buttonWithButtonType ButtonType_Primary
    >>= buttonWithOnClick dat onClick
    >>= buttonDom
  domCreateBody >>= domWithChild label >>= domWithChild button

onClick :: DataModel -> CallbackInfo -> (DataModel, Update)
onClick model _ = (model { counter = counter model + 1 }, Update_RefreshDom)

main :: IO ()
main = do
  dat <- refAnyCreate (DataModel 5)
  window <- windowCreateOptionsCreate layout
  appConfigCreate >>= appCreate dat >>= appRun window
