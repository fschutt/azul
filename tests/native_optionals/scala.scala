// Scala: the Java binding's classes with Scala's own optional on top. An
// Option parameter takes the value or `null` (`opt.orNull`), a field or a
// downcast gives a java Optional that `.toScala` turns into an `Option`, and
// an Err is an exception (inside a callback the binding logs it and the
// callback does nothing). Built and run by scripts/native_optionals_smoke.sh.
import com.azul._
import scala.jdk.OptionConverters._

class Model {
  var folder: String = ""
  var link: String = "no scheme"
}

object NativeOptionalsScala {
  // The guide's file-dialog example, as written there (hello-world/scala.md).
  def onChooseFolder(model: Model, info: CallbackInfo): Update = {
    FileDialog.openDirectory("Choose a folder", null, model, onPickFolder(_, _, _))
    Update.DoNothing
  }

  def onPickFolder(model: Model, info: CallbackInfo, result: RefAny): Update = {
    val path = for {
      picked <- FileOpenResult.downcast(result).toScala
      path <- picked.getPath.toScala
    } yield path
    path match {
      case Some(p) =>
        model.folder = p.getInner
        Update.RefreshDom
      case None => Update.DoNothing // not a folder pick, or cancelled
    }
  }

  def onOpenLink(model: Model, info: CallbackInfo): Update = {
    val url = Url.parse(model.link) // an Err throws: logged, the callback does nothing
    Update.RefreshDom
  }

  def check(ok: Boolean, what: String): Unit =
    if (!ok) { System.err.println(s"FAIL: $what"); sys.exit(1) }

  def main(args: Array[String]): Unit = {
    val icons = IconVariants.createDefault()
    check(icons.getLight.toScala.isEmpty, "a default IconVariants has no light icon")
    icons.setLight(Some("light.png").orNull)
    check(icons.getLight.toScala.contains("light.png"), "Some in, Some out")
    icons.setLight(None.orNull)
    check(icons.getLight.toScala.isEmpty, "None in, None out")

    check(scala.util.Try(Url.parse("https://azul.rs/guide")).isSuccess, "Url.parse Ok")
    check(scala.util.Try(Url.parse("no scheme")).isFailure, "Url.parse Err throws")

    val button = Button.create("Open").withOnClick(new Model, onOpenLink(_, _))
    println("scala native optionals: ok")
  }
}
