module Main (main) where

import Styles

main :: IO ()
main = do
  styleBtnValue <- styleBtn
  putStrLn ("styleBtn: " ++ show (length styleBtnValue) ++ " properties")
  pure ()
