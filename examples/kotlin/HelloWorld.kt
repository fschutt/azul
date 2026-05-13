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

import com.sun.jna.Memory
import com.sun.jna.Pointer
import com.sun.jna.Structure

class MyDataModel(var counter: Int)
private val MODEL = MyDataModel(5)

private fun str(s: kotlin.String): AzString.ByValue {
    val bytes = s.toByteArray(Charsets.UTF_8)
    val mem = Memory(bytes.size.toLong())
    mem.write(0, bytes, 0, bytes.size)
    return AzulNativeStr.INSTANCE.AzString_fromUtf8(mem, bytes.size.toLong())
}

private val onClick = AzulNativeManaged.CallbackInvokerCallback { _, dataPtr, _, outPtr ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    val result = if (m is MyDataModel) { m.counter += 1; AzUpdate.RefreshDom.value }
                 else AzUpdate.DoNothing.value
    outPtr!!.setInt(0, result)
}

private val layout = AzulNativeManaged.LayoutCallbackInvokerCallback { _, dataPtr, _, outPtr ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    if (m !is MyDataModel) {
        val empty = AzulNativeDom.INSTANCE.AzDom_createBody()
        empty.write()
        outPtr!!.write(0, empty.pointer.getByteArray(0, empty.size()), 0, empty.size())
        return@LayoutCallbackInvokerCallback
    }
    val label = AzulNativeDom.INSTANCE.AzDom_withChild(
        AzulNativeDom.INSTANCE.AzDom_withCss(AzulNativeDom.INSTANCE.AzDom_createDiv(), str("font-size: 32px;")),
        AzulNativeDom.INSTANCE.AzDom_createText(str(m.counter.toString())))
    val btn = AzulNativeWidgets.INSTANCE.AzButton_withOnClick(
        AzulNativeWidgets.INSTANCE.AzButton_withButtonType(
            AzulNativeWidgets.INSTANCE.AzButton_create(str("Increase counter")), AzButtonType.Primary.value),
        AzulHostInvoker.refanyCreate(m), AzulHostInvoker.registerCallback(onClick))
    val body = AzulNativeDom.INSTANCE.AzDom_withChild(
        AzulNativeDom.INSTANCE.AzDom_withChild(AzulNativeDom.INSTANCE.AzDom_createBody(), label),
        AzulNativeWidgets.INSTANCE.AzButton_dom(btn))
    body.write()
    outPtr!!.write(0, body.pointer.getByteArray(0, body.size()), 0, body.size())
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
