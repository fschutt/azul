// Kotlin: `null` where an Option is asked for, a nullable back from fields
// and downcasts (so `?.` and `?: return` read the whole chain), an exception
// for an Err (inside a callback the binding logs it and the callback does
// nothing), and the resume callback as a typed function reference. Built
// and run by scripts/native_optionals_smoke.sh.
package com.azul

class Model {
    var folder = ""
    var link = "no scheme"
}

// The guide's file-dialog example, as written there (hello-world/kotlin.md).
fun onChooseFolder(model: Model, info: CallbackInfo): Update {
    FileDialog.openDirectory("Choose a folder", null, model, ::onPickFolder)
    return Update.DoNothing
}

fun onPickFolder(model: Model, info: CallbackInfo, result: RefAny): Update {
    val path = FileOpenResult.downcast(result)?.path
        ?: return Update.DoNothing // not a folder pick, or cancelled
    model.folder = path.inner
    return Update.RefreshDom
}

fun onOpenLink(model: Model, info: CallbackInfo): Update {
    val url = Url.parse(model.link) // an Err throws: logged, the callback does nothing
    return Update.RefreshDom
}

fun check(ok: Boolean, what: String) {
    if (!ok) {
        System.err.println("FAIL: $what")
        kotlin.system.exitProcess(1)
    }
}

fun main() {
    // A field: null in, null out; a value in, the value out.
    val icons = IconVariants.createDefault()
    check(icons.light == null, "a default IconVariants has no light icon")
    icons.light = "light.png"
    check(icons.light == "light.png", "Some in, Some out")
    icons.light = null
    check(icons.light == null, "None in, None out")

    // A class payload: a Dom moved in, a Dom read back, null clears it.
    val view = VirtualViewReturn.createDefault()
    check(view.dom == null, "a default VirtualViewReturn has no Dom")
    view.dom = Dom.createBody()
    check(view.dom != null, "a Dom moved into an Option field reads back")
    view.dom = null
    check(view.dom == null, "null clears the field")

    // A result: Ok is the value, Err an exception.
    check(runCatching { Url.parse("https://azul.rs/guide") }.isSuccess, "Url.parse Ok")
    check(runCatching { Url.parse("no scheme") }.isFailure, "Url.parse Err throws")

    Button.create("Open").withOnClick(Model(), ::onOpenLink)
    println("kotlin native optionals: ok")
}
