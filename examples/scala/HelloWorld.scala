// examples/scala/HelloWorld.scala — idiomatic Scala port of the hello-world
// counter, riding on the Java codegen's `com.azul.*` JNA bindings.
//
// Everything goes through the non-prefixed wrapper classes (Dom, Button,
// App, AppConfig, WindowCreateOptions) plus the AzulHostInvoker helpers.
// The typed `AzulHostInvoker.LayoutCallback` SAM returns a `Dom` directly;
// the host-invoker bridge does the struct-byte splice internally, so user
// code never touches `Structure.newInstance` / `getByteArray`. Enum types
// are unprefixed too (Update, ButtonType) — nothing Az-prefixed remains
// in user code.
//
// Build the JNA bindings, then run with the Scala 3 runner:
//   javac -cp jna.jar -d classes azul-java/*.java
//   scala run HelloWorld.scala --class-path classes:jna.jar \
//       --java-opt -Djna.library.path=. --java-opt -XstartOnFirstThread
//
// macOS requires `-XstartOnFirstThread` (Cocoa main-thread rule).

package com.azul

import com.sun.jna.Pointer

object HelloWorld {

  class MyDataModel(var counter: Int)
  private val MODEL = new MyDataModel(5)

  private val ON_CLICK: AzulNativeManaged.ButtonOnClickCallbackInvokerCallback =
    new AzulNativeManaged.ButtonOnClickCallbackInvokerCallback {
      override def invoke(id: Long, dataPtr: Pointer, infoPtr: Pointer, outPtr: Pointer): Unit =
        AzulHostInvoker.refanyGet(dataPtr) match {
          case m: MyDataModel =>
            m.counter += 1
            outPtr.setInt(0, Update.RefreshDom.value)
          case _ =>
            outPtr.setInt(0, Update.DoNothing.value)
        }
    }

  private val LAYOUT: AzulHostInvoker.LayoutCallback =
    new AzulHostInvoker.LayoutCallback {
      override def invoke(id: Long, dataPtr: Pointer, infoPtr: Pointer): Dom =
        AzulHostInvoker.refanyGet(dataPtr) match {
          case m: MyDataModel =>
            val label = Dom.createDiv()
              .withCss("font-size: 32px;")
              .withChild(Dom.createText(String.valueOf(m.counter)))
            val buttonDom = Button.create("Increase counter")
              .withButtonType(ButtonType.Primary.value)
              .onClick(m, ON_CLICK)
              .dom()
            Dom.createBody()
              .withChild(label)
              .withChild(buttonDom)
          case _ =>
            Dom.createBody()
        }
    }

  def main(args: Array[String]): Unit = {
    val app = App.create(AzulHostInvoker.refanyWrap(MODEL), AppConfig.create())
    try app.run(WindowCreateOptions.create(LAYOUT))
    finally app.close()
  }
}
