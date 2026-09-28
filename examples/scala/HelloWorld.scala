package com.azul

import scala.util.Using

class Counter {
  var count: Int = 5
}

object HelloWorld {
  def main(args: Array[String]): Unit =
    Using.resource(App.create(new Counter, layout(_, _))) { app =>
      val options = WindowCreateOptions.create()
      app.run(options)
    }

  def layout(data: Counter, info: LayoutCallbackInfo): Dom = {
    val countStr = data.count.toString
    val btn = Button.create("Increase counter")
      .withOnClick(data, onClick(_, _))

    Dom.createBody()
      .withChild(Dom.createPWithText(countStr))
      .withChild(btn.dom())
  }

  def onClick(data: Counter, info: CallbackInfo): Update = {
    data.count += 1
    Update.RefreshDom
  }
}
