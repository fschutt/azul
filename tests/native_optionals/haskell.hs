-- Haskell: 'Maybe' where an Option is asked for and back from fields and
-- downcasts, 'Either' for a Result, and the guide's handlers written with
-- 'MaybeT' / 'ExceptT' through 'orDoNothing' / 'orDoNothingOnError'. Built
-- and run by scripts/native_optionals_smoke.sh.
module Main where

import Azul
import qualified Azul.Dom as Dom
import qualified Azul.FileDialog as FileDialog
import qualified Azul.FileOpenResult as FileOpenResult
import qualified Azul.AzFilePath as FilePath
import qualified Azul.IconVariants as IconVariants
import qualified Azul.Url as Url
import qualified Azul.VirtualViewReturn as VirtualViewReturn
import Control.Monad (unless)
import Control.Monad.IO.Class (liftIO)
import Data.Either (isLeft, isRight)
import Data.Maybe (isJust, isNothing)
import System.Exit (exitFailure)

data Model = Model { folder :: String, link :: String }

-- The guide's file-dialog example, as written there (hello-world/haskell.md).
onChooseFolder :: Model -> CallbackInfo -> IO (Model, Update)
onChooseFolder model _ = do
  _ <- FileDialog.openDirectoryWithModel "Choose a folder" Nothing onPickFolder
  pure (model, Update_DoNothing)

onPickFolder :: Model -> CallbackInfo -> RefAny -> IO (Model, Update)
onPickFolder model _ result = orDoNothing model $ do
  picked <- MaybeT (FileOpenResult.downcast result)
  path <- MaybeT (FileOpenResult.getPath picked) -- Nothing when cancelled
  name <- liftIO (FilePath.getInner path)
  pure (model { folder = name }, Update_RefreshDom)

onOpenLink :: Model -> CallbackInfo -> IO (Model, Update)
onOpenLink model _ = orDoNothingOnError model $ do
  _url <- ExceptT (Url.parse (link model)) -- an Err is logged, the handler does nothing
  pure (model, Update_RefreshDom)

check :: Bool -> String -> IO ()
check ok what = unless ok $ do
  putStrLn ("FAIL: " ++ what)
  exitFailure

main :: IO ()
main = do
  -- A field: Nothing in, Nothing out; Just in, Just out.
  icons <- IconVariants.default'
  IconVariants.getLight icons >>= \l -> check (isNothing l) "a default IconVariants has no light icon"
  IconVariants.setLight (Just "light.png") icons
  IconVariants.getLight icons >>= \l -> check (l == Just "light.png") "Just in, Just out"
  IconVariants.setLight Nothing icons
  IconVariants.getLight icons >>= \l -> check (isNothing l) "Nothing in, Nothing out"

  -- A class payload: a Dom moved in, a Dom read back, Nothing clears it.
  view <- VirtualViewReturn.default'
  VirtualViewReturn.getDom view >>= \d -> check (isNothing d) "a default VirtualViewReturn has no Dom"
  body <- Dom.createBody
  VirtualViewReturn.setDom (Just body) view
  VirtualViewReturn.getDom view >>= \d -> check (isJust d) "a Dom moved into an Option field reads back"
  VirtualViewReturn.setDom Nothing view
  VirtualViewReturn.getDom view >>= \d -> check (isNothing d) "Nothing clears the field"

  -- A result: Right is Ok, Left is Err.
  Url.parse "https://azul.rs/guide" >>= \r -> check (isRight r) "Url.parse Ok"
  Url.parse "no scheme" >>= \r -> check (isLeft r) "Url.parse Err"

  -- The guards on their own.
  (_, u1) <- orDoNothing (Model "" "") (MaybeT (pure Nothing))
  check (u1 == Update_DoNothing) "orDoNothing answers DoNothing for Nothing"
  (_, u2) <- orDoNothingOnError (Model "" "") (ExceptT (pure (Left "no")))
  check (u2 == Update_DoNothing) "orDoNothingOnError answers DoNothing for Left"
  let _ = (onChooseFolder, onPickFolder, onOpenLink)
  putStrLn "haskell native optionals: ok"
