// examples/kotlin/HelloWorld.kt — Python-quality Kotlin port.
//
// Uses two smart factories: the typed `LayoutCallback` SAM that
// returns a `Dom` directly (CC-2), plus
// `WindowCreateOptions.create(LayoutCallback)` that hides the
// AzLayoutCallback ↔ wco `window_state.layout_callback` byte splice.
// User code never reaches for `Structure.newInstance` /
// `outPtr.write` / any JNA pointer-byte ceremony.
//
// Build:  kotlinc -J-Xmx4g -cp $JNA_JAR Azul.kt HelloWorld.kt -include-runtime -d hello-world.jar
// Run:    DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
//             -cp hello-world.jar:$JNA_JAR com.azul.HelloWorldKt
//
// macOS requires `-XstartOnFirstThread` (Cocoa main-thread rule).

package com.azul

import com.sun.jna.Pointer
import com.sun.jna.Structure

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
    // Smart factory hides the host-invoker register + bytes-splice.
    val wco = WindowCreateOptions.create(layout)
    val rawWco = Structure.newInstance(AzWindowCreateOptions.ByValue::class.java, wco.rawPointer())
    rawWco.read()
    val data = AzulHostInvoker.refanyCreate(MODEL)
    val app = AzulNativeApp.INSTANCE.AzApp_create(data, AzulNativeApp.INSTANCE.AzAppConfig_create())
    app.write()
    AzulNativeApp.INSTANCE.AzApp_run(app.pointer, rawWco)
}
