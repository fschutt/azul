// examples/kotlin/HelloWorld.kt — Python-quality Kotlin port.
//
// Uses the typed `LayoutCallback` SAM that returns a `Dom` directly
// (CC-2) and the wrapper-class `App` API (CC-5): no JNA byte splice,
// no Marshal/AllocHGlobal-style pointer dance.
//
// Build:  kotlinc -J-Xmx4g -cp $JNA_JAR Azul.kt HelloWorld.kt -include-runtime -d hello-world.jar
// Run:    DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
//             -cp hello-world.jar:$JNA_JAR com.azul.HelloWorldKt
//
// macOS requires `-XstartOnFirstThread` (Cocoa main-thread rule).

package com.azul

import com.sun.jna.Pointer

class MyDataModel(var counter: Int)
private val MODEL = MyDataModel(5)

private val onClick = AzulNativeManaged.CallbackInvokerCallback { _, dataPtr, _, outPtr ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    val result = if (m is MyDataModel) { m.counter += 1; AzUpdate.RefreshDom.value }
                 else AzUpdate.DoNothing.value
    outPtr!!.setInt(0, result)
}

// Typed layout callback: returns Dom directly. The bridge in
// AzulHostInvoker.registerLayoutCallback(LayoutCallback) does the
// AzDom-byte splice into outPtr internally.
private val layout = AzulHostInvoker.LayoutCallback { _, dataPtr, _ ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    if (m !is MyDataModel) {
        Dom.createBody()
    } else {
        val label = Dom.createDiv()
            .withCss("font-size: 32px;")
            .withChild(Dom.createText(m.counter.toString()))
        val buttonDom = Dom(
            Button.create("Increase counter")
                .withButtonType(AzButtonType.Primary.value)
                .onClick(m, onClick)
                .dom()
                .pointer)
        Dom.createBody()
            .withChild(label)
            .withChild(buttonDom)
    }
}

fun main() {
    App.create(AzulHostInvoker.refanyWrap(MODEL), AppConfig.create()).use { app ->
        app.run(WindowCreateOptions.create(layout))
    }
}
