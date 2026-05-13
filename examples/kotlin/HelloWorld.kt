// examples/kotlin/HelloWorld.kt — Python-quality Kotlin port.
//
// Uses `WindowCreateOptions.create(LAYOUT)` smart factory; user code
// never has to manage the JNA `Pointer.write` splice for the embedded
// layout_callback.
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

private fun writeDom(outPtr: Pointer, dom: Dom) {
    val raw = Structure.newInstance(AzDom.ByValue::class.java, dom.rawPointer()) as AzDom.ByValue
    raw.read()
    outPtr.write(0, raw.pointer.getByteArray(0, raw.size()), 0, raw.size())
}

private val layout = AzulNativeManaged.LayoutCallbackInvokerCallback { _, dataPtr, _, outPtr ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    if (m !is MyDataModel) {
        writeDom(outPtr!!, Dom.createBody())
        return@LayoutCallbackInvokerCallback
    }
    val label = Dom.createDiv()
        .withCss("font-size: 32px;")
        .withChild(Dom.createText(m.counter.toString()))
    val buttonDom = Dom(
        Button.create("Increase counter")
            .withButtonType(AzButtonType.Primary.value)
            .onClick(m, onClick)
            .dom()
            .pointer)
    val body = Dom.createBody()
        .withChild(label)
        .withChild(buttonDom)
    writeDom(outPtr!!, body)
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
