module Main where

import Azul

newtype DataModel = DataModel { counter :: Int }

layout :: RefAny -> DataModel -> LayoutCallbackInfo -> IO Dom
layout dat model _ = do
  label <- domCreatePWithText (show (counter model)) >>= domWithCss "font-size: 32px; margin: 0;"
  button <- buttonCreate "Increase counter"
    >>= buttonWithButtonType ButtonType_Primary
    >>= buttonWithOnClick dat onClick
    >>= buttonDom
  domCreateBody >>= domWithChild label >>= domWithChild button

onClick :: RefAny -> CallbackInfo -> IO Update
onClick dat _ = refAnyUpdate dat (\m -> m { counter = counter m + 1 }) Update_RefreshDom

main :: IO ()
main = do
  dat <- refAnyCreate (DataModel 5)
  window <- windowCreateOptionsCreate layout
  appConfigCreate >>= appCreate dat >>= appRun window
