// examples/kotlin/HelloWorld.kt
//
// Kotlin port of examples/c/hello-world.c. Same data model (a counter),
// same behaviour (mouse click increments, layout rebuilds the DOM).
// Callbacks go through libazul's host-invoker plumbing — JNA never has
// to synthesize a struct-by-value trampoline for user code.
//
// Build + run (macOS):
//   kotlinc -J-Xmx4g -cp $JNA_JAR Azul.kt HelloWorld.kt -include-runtime -d hello-world.jar
//   DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
//       -cp hello-world.jar:$JNA_JAR com.azul.HelloWorldKt
//
// Native bindings live in per-api.json-module JNA interfaces:
//   AzulNativeApp     — App, AppConfig
//   AzulNativeDom     — Dom, Callback, NodeData, …
//   AzulNativeWindow  — WindowCreateOptions, FullWindowState, …
//   AzulNativeWidgets — Button, CheckBox, TextInput, …
//   AzulNativeStr     — String
//   AzulNativeCallbacks — RefAny, LayoutCallback, …
//   AzulNativeManaged — host-invoker plumbing
// (and ~20 more modules — see api.json for the full list).
//
// Note: macOS requires `-XstartOnFirstThread` so libazul's NSApplication
// loop can pump on the JVM main thread.

package com.azul

import com.sun.jna.Memory
import com.sun.jna.Pointer

class MyDataModel(var counter: Int)

private val MODEL = MyDataModel(5)

private fun str(s: kotlin.String): AzString.ByValue {
    val bytes = s.toByteArray(Charsets.UTF_8)
    val mem = Memory(bytes.size.toLong())
    mem.write(0, bytes, 0, bytes.size)
    return AzulNativeStr.INSTANCE.AzString_fromUtf8(mem, bytes.size.toLong())
}

private val onClickInvoker = AzulNativeManaged.CallbackInvokerCallback { _, dataPtr, _, outPtr ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    val result = if (m is MyDataModel) { m.counter += 1; 1 } else 0
    outPtr!!.setInt(0, result)
}

private val layoutInvoker = AzulNativeManaged.LayoutCallbackInvokerCallback { _, dataPtr, _, outPtr ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    if (m !is MyDataModel) {
        val empty = AzulNativeDom.INSTANCE.AzDom_createBody()
        empty.write()
        outPtr!!.write(0, empty.pointer.getByteArray(0, empty.size()), 0, empty.size())
        return@LayoutCallbackInvokerCallback
    }

    val clickCb = AzulHostInvoker.registerCallback(onClickInvoker)
    val clickData = AzulHostInvoker.refanyCreate(m)

    val counterText = AzulNativeDom.INSTANCE.AzDom_createText(str(m.counter.toString()))
    val label = AzulNativeDom.INSTANCE.AzDom_withChild(
        AzulNativeDom.INSTANCE.AzDom_withCss(AzulNativeDom.INSTANCE.AzDom_createDiv(), str("font-size: 32px;")),
        counterText,
    )

    val btn = AzulNativeWidgets.INSTANCE.AzButton_withOnClick(
        AzulNativeWidgets.INSTANCE.AzButton_withButtonType(
            AzulNativeWidgets.INSTANCE.AzButton_create(str("Increase counter")),
            AzButtonType.Primary.value,
        ),
        clickData,
        clickCb,
    )

    val body = AzulNativeDom.INSTANCE.AzDom_withChild(
        AzulNativeDom.INSTANCE.AzDom_withChild(AzulNativeDom.INSTANCE.AzDom_createBody(), label),
        AzulNativeWidgets.INSTANCE.AzButton_dom(btn),
    )
    body.write()
    outPtr!!.write(0, body.pointer.getByteArray(0, body.size()), 0, body.size())
}

fun main() {
    val data = AzulHostInvoker.refanyCreate(MODEL)
    val layoutCb = AzulHostInvoker.registerLayoutCallback(layoutInvoker)

    val wco = AzulNativeWindow.INSTANCE.AzWindowCreateOptions_default()
    // JNA's nested-struct field assignment is a Java reference swap,
    // not a byte copy. Flush layoutCb bytes into the wco.window_state
    // layout_callback storage directly so libazul sees them.
    layoutCb.write()
    wco.write()
    val cbBytes = layoutCb.pointer.getByteArray(0, layoutCb.size())
    wco.window_state.layout_callback.pointer.write(0, cbBytes, 0, cbBytes.size)
    wco.read()

    val cfg = AzulNativeApp.INSTANCE.AzAppConfig_create()
    val app = AzulNativeApp.INSTANCE.AzApp_create(data, cfg)
    app.write()
    AzulNativeApp.INSTANCE.AzApp_run(app.pointer, wco)
}
