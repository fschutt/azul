package com.azul

class Counter {
    var count: Int = 0
}

// Top-level functions don't need @JvmStatic or objects!
fun layout(data: Counter, info: AzLayoutCallbackInfo): Dom {
    val countStr = "Count: ${data.count}"
    val btn = Button.create(countStr)
        .withOnClick(data, ::onClick)
    
    return Dom.createBody()
        .withChild(btn.dom())
}

fun onClick(data: Counter, info: AzCallbackInfo): Update {
    data.count++
    return Update.RefreshDom
}

fun main(args: Array<String>) {
    App.create(Counter(), ::layout).use { app ->
        val options = WindowCreateOptions.create()
        app.run(options)
    }
}
