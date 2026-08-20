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
              .withChild(Dom.createTextDoNotUseWithoutBlockLevelWrapper(String.valueOf(m.counter)))
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
