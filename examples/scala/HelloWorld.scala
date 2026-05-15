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

  private def writeDom(outPtr: Pointer, dom: Dom): Unit = {
    val raw = Structure.newInstance(classOf[AzDom.ByValue], dom.rawPointer())
    raw.read()
    outPtr.write(0, raw.getPointer().getByteArray(0, raw.size()), 0, raw.size())
  }

  private val LAYOUT: AzulNativeManaged.LayoutCallbackInvokerCallback =
    new AzulNativeManaged.LayoutCallbackInvokerCallback {
      override def invoke(id: Long, dataPtr: Pointer, infoPtr: Pointer, outPtr: Pointer): Unit =
        AzulHostInvoker.refanyGet(dataPtr) match {
          case m: MyDataModel =>
            val label = Dom.createDiv()
              .withCss("font-size: 32px;")
              .withChild(Dom.createText(String.valueOf(m.counter)))
            val buttonDom = new Dom(
              Button.create("Increase counter")
                .withButtonType(AzButtonType.Primary.value)
                .onClick(m, ON_CLICK)
                .dom()
                .getPointer())
            writeDom(outPtr, Dom.createBody().withChild(label).withChild(buttonDom))
          case _ =>
            writeDom(outPtr, Dom.createBody())
        }
    }

  def main(args: Array[String]): Unit = {
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
