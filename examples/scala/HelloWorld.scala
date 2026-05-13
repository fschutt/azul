// examples/scala/HelloWorld.scala — Python-quality Scala port.
//
// Rides on the Java codegen's `com.azul.*` JNA bindings. The smart
// `WindowCreateOptions.create(LAYOUT_INVOKER)` factory hides the
// host-invoker plumbing; user code never has to touch `getByteArray`
// or splice bytes through `Pointer.write`. Builds at ~50 LOC.
//
// Build:  scalac -cp ../java/target/classes:$JNA_JAR HelloWorld.scala -d HelloWorld.jar
// Run:    DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
//             -cp HelloWorld.jar:../java/target/classes:$JNA_JAR:$SCALA_LIB:$SCALA3_LIB \
//             com.azul.HelloWorld
//
// macOS requires `-XstartOnFirstThread` (Cocoa main-thread rule).

package com.azul

import com.sun.jna.{Pointer, Structure}

object HelloWorld {

  class MyDataModel(var counter: Int)
  private val MODEL = new MyDataModel(5)

  private def str(s: java.lang.String): AzString.ByValue = {
    val bytes = s.getBytes(java.nio.charset.StandardCharsets.UTF_8)
    val mem = new com.sun.jna.Memory(bytes.length.toLong)
    mem.write(0, bytes, 0, bytes.length)
    AzulNativeStr.AzString_fromUtf8(mem, bytes.length.toLong)
  }

  private val ON_CLICK: AzulNativeManaged.CallbackInvokerCallback =
    new AzulNativeManaged.CallbackInvokerCallback {
      override def invoke(id: Long, dataPtr: Pointer, infoPtr: Pointer, outPtr: Pointer): Unit =
        AzulHostInvoker.refanyGet(dataPtr) match {
          case m: MyDataModel =>
            m.counter += 1
            outPtr.setInt(0, AzUpdate.RefreshDom.value)
          case _ =>
            outPtr.setInt(0, AzUpdate.DoNothing.value)
        }
    }

  private val LAYOUT: AzulNativeManaged.LayoutCallbackInvokerCallback =
    new AzulNativeManaged.LayoutCallbackInvokerCallback {
      override def invoke(id: Long, dataPtr: Pointer, infoPtr: Pointer, outPtr: Pointer): Unit =
        AzulHostInvoker.refanyGet(dataPtr) match {
          case m: MyDataModel =>
            val label = AzulNativeDom.AzDom_withChild(
              AzulNativeDom.AzDom_withCss(AzulNativeDom.AzDom_createDiv(), str("font-size: 32px;")),
              AzulNativeDom.AzDom_createText(str(java.lang.String.valueOf(m.counter))))
            val btn = AzulNativeWidgets.AzButton_withOnClick(
              AzulNativeWidgets.AzButton_withButtonType(
                AzulNativeWidgets.AzButton_create(str("Increase counter")), AzButtonType.Primary.value),
              AzulHostInvoker.refanyCreate(m), AzulHostInvoker.registerCallback(ON_CLICK))
            val body = AzulNativeDom.AzDom_withChild(
              AzulNativeDom.AzDom_withChild(AzulNativeDom.AzDom_createBody(), label),
              AzulNativeWidgets.AzButton_dom(btn))
            body.write()
            outPtr.write(0, body.getPointer().getByteArray(0, body.size()), 0, body.size())
          case _ =>
            val empty = AzulNativeDom.AzDom_createBody()
            empty.write()
            outPtr.write(0, empty.getPointer().getByteArray(0, empty.size()), 0, empty.size())
        }
    }

  def main(args: Array[java.lang.String]): Unit = {
    // Smart factory: hides the host-invoker register + bytes-splice
    // (compare with the pre-rewrite version's ~6 lines of boilerplate).
    val wco = WindowCreateOptions.create(LAYOUT)
    val rawWco = Structure.newInstance(classOf[AzWindowCreateOptions.ByValue], wco.rawPointer())
    rawWco.read()
    val app = AzulNativeApp.AzApp_create(AzulHostInvoker.refanyCreate(MODEL), AzulNativeApp.AzAppConfig_create())
    app.write()
    AzulNativeApp.AzApp_run(app.getPointer(), rawWco)
  }
}
