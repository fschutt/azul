// examples/scala/HelloWorld.scala
//
// Scala port of examples/java/HelloWorld.java. Rides on the Java
// codegen's `com.azul.*` JNA bindings (per-api.json-module
// AzulNative<Module> interfaces + AzulHostInvoker + struct classes).
// Scala/Java interop is transparent — same JVM, same JNA proxies.
//
// Build:
//     scalac -cp ../java/target/classes:$JNA_JAR HelloWorld.scala -d HelloWorld.jar
//
// Run (macOS):
//     DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
//         -cp HelloWorld.jar:../java/target/classes:$JNA_JAR \
//         com.azul.HelloWorld
//
// macOS requires `-XstartOnFirstThread` because libazul pumps
// NSApplication on the calling thread, which must be the process's
// main thread on Cocoa.

package com.azul

import com.sun.jna.Pointer

object HelloWorld {

  class MyDataModel(var counter: Int)

  private val MODEL = new MyDataModel(5)

  // Copy a UTF-8 string into an AzString. AzString_fromUtf8 takes
  // its own copy inside, so the JNA Memory buffer can be released
  // after the call.
  //
  // Inside `package com.azul`, an unqualified `String` resolves to
  // the codegen's `com.azul.String` wrapper, not `java.lang.String`.
  // Qualify everywhere we want the JVM string.
  private def str(s: java.lang.String): AzString.ByValue = {
    val bytes = s.getBytes(java.nio.charset.StandardCharsets.UTF_8)
    val mem = new com.sun.jna.Memory(bytes.length.toLong)
    mem.write(0, bytes, 0, bytes.length)
    AzulNativeStr.AzString_fromUtf8(mem, bytes.length.toLong)
  }

  // CallbackInvokerCallback / LayoutCallbackInvokerCallback are JNA
  // SAM interfaces — Scala 3 accepts them as anonymous-class instances.

  private val ON_CLICK_INVOKER: AzulNativeManaged.CallbackInvokerCallback =
    new AzulNativeManaged.CallbackInvokerCallback {
      override def invoke(id: Long, dataPtr: Pointer, infoPtr: Pointer, outPtr: Pointer): Unit = {
        val m = AzulHostInvoker.refanyGet(dataPtr)
        var result = 0 // AzUpdate.DoNothing
        m match {
          case model: MyDataModel =>
            model.counter += 1
            result = 1 // AzUpdate.RefreshDom
          case _ =>
        }
        outPtr.setInt(0, result)
      }
    }

  private val LAYOUT_INVOKER: AzulNativeManaged.LayoutCallbackInvokerCallback =
    new AzulNativeManaged.LayoutCallbackInvokerCallback {
      override def invoke(id: Long, dataPtr: Pointer, infoPtr: Pointer, outPtr: Pointer): Unit = {
        val recovered = AzulHostInvoker.refanyGet(dataPtr)
        recovered match {
          case m: MyDataModel =>
            val clickCb = AzulHostInvoker.registerCallback(ON_CLICK_INVOKER)
            val clickData = AzulHostInvoker.refanyCreate(m)

            // <div font-size:32px><text>{counter}</text></div>
            val counterText =
              AzulNativeDom.AzDom_createText(str(java.lang.String.valueOf(m.counter)))
            val label =
              AzulNativeDom.AzDom_withChild(
                AzulNativeDom.AzDom_withCss(
                  AzulNativeDom.AzDom_createDiv(),
                  str("font-size: 32px;")
                ),
                counterText
              )

            // <button>Increase counter</button>
            val btn = AzulNativeWidgets.AzButton_withOnClick(
              AzulNativeWidgets.AzButton_withButtonType(
                AzulNativeWidgets.AzButton_create(str("Increase counter")),
                AzButtonType.Primary.value
              ),
              clickData,
              clickCb
            )

            val body = AzulNativeDom.AzDom_withChild(
              AzulNativeDom.AzDom_withChild(
                AzulNativeDom.AzDom_createBody(),
                label
              ),
              AzulNativeWidgets.AzButton_dom(btn)
            )

            body.write()
            val bytes = body.getPointer().getByteArray(0, body.size())
            outPtr.write(0, bytes, 0, bytes.length)

          case _ =>
            val empty = AzulNativeDom.AzDom_createBody()
            empty.write()
            outPtr.write(0, empty.getPointer().getByteArray(0, empty.size()), 0, empty.size())
        }
      }
    }

  def main(args: Array[java.lang.String]): Unit = {
    val data = AzulHostInvoker.refanyCreate(MODEL)
    val layoutCb = AzulHostInvoker.registerLayoutCallback(LAYOUT_INVOKER)

    val wco = AzulNativeWindow.AzWindowCreateOptions_default()
    // JNA's nested-struct field assignment is a reference swap, not
    // a byte copy. Flush layoutCb bytes into the wco's existing
    // layout_callback memory directly.
    layoutCb.write()
    wco.write()
    val cbBytes = layoutCb.getPointer().getByteArray(0, layoutCb.size())
    wco.window_state.layout_callback.getPointer().write(0, cbBytes, 0, cbBytes.length)
    wco.read()

    val cfg = AzulNativeApp.AzAppConfig_create()
    val app = AzulNativeApp.AzApp_create(data, cfg)
    app.write()
    AzulNativeApp.AzApp_run(app.getPointer(), wco)
  }
}
