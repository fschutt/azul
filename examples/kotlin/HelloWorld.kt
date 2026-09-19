package com.azul

class Counter {
    var count: Int = 5
}

fun layout(data: Counter, info: LayoutCallbackInfo): Dom {
    val countStr = "${data.count}"
    val btn = Button.create("Increase counter")
        .withOnClick(data, ::onClick)
    
    return Dom.createBody()
        .withChild(Dom.createPWithText(countStr))
        .withChild(btn.dom())
}

fun onClick(data: Counter, info: CallbackInfo): Update {
    data.count++
    return Update.RefreshDom
}

fun main(args: Array<String>) {
    App.create(Counter(), ::layout).use { app ->
        val options = WindowCreateOptions.create()
        app.run(options)
    }
}
