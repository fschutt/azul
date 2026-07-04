// kotlinc -cp $JNA_JAR Azul.kt HelloWorld.kt -include-runtime -d hello-world.jar && java -Djna.library.path=. -cp hello-world.jar:$JNA_JAR com.azul.HelloWorldKt

package com.azul

import com.sun.jna.Pointer

class MyDataModel(var counter: Int)
private val MODEL = MyDataModel(5)

private val onClick = AzulNativeManaged.ButtonOnClickCallbackInvokerCallback { _, dataPtr, _, outPtr ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    val result = if (m is MyDataModel) { m.counter += 1; Update.RefreshDom.value }
                 else Update.DoNothing.value
    outPtr!!.setInt(0, result)
}

private val layout = AzulHostInvoker.LayoutCallback { _, dataPtr, _ ->
    val m = AzulHostInvoker.refanyGet(dataPtr)
    if (m !is MyDataModel) {
        Dom.createBody()
    } else {
        val label = Dom.createDiv()
            .withCss("font-size: 32px;")
            .withChild(Dom.createText(m.counter.toString()))
        val buttonDom = Button.create("Increase counter")
            .withButtonType(ButtonType.Primary.value)
            .onClick(m, onClick)
            .dom()
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
