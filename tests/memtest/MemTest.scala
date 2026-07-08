// Memory test for the azul Scala (JNA, com.azul.*) binding. See tests/memtest/README.md.
//
// The harness (scripts/run_memtest.sh) does the measuring: it runs this twice
// with a small and a large AZ_MEMTEST_N and compares peak RSS (RSS that scales
// with N is a LEAK), and watches for any crash (double-free / UAF). This file
// only exercises the create/consume/DROP paths N times and exits 0. No event
// loop (App.run needs a display and hangs headless).
//
// Build/run like examples/scala/HelloWorld.scala (JNA, -Djna.library.path=.),
// but against this file instead of HelloWorld.scala.

package com.azul

object MemTest {

  class MyDataModel(var counter: Int)
  private val MODEL = new MyDataModel(5)

  def main(args: Array[String]): Unit = {
    val env = System.getenv("AZ_MEMTEST_N")
    val n = if (env != null && !env.trim.isEmpty) env.trim.toInt else 200000

    // 1. Consume-by-value DROP path once: App.create consumes AppConfig (whose
    //    nested SystemStyle was one of the bitwise-clone/double-free types).
    //    close() the App instead of running an event loop.
    val app = App.create(AzulHostInvoker.refanyWrap(MODEL), AppConfig.create())
    app.close() // no run(): would need a display and hang headless.

    // 2. Leak loop: create/close a droppable object N times. AutoCloseable
    //    close() frees the native data every iteration; we do NOT rely on
    //    GC/finalizer timing, which would mask a leak.
    var i = 0
    while (i < n) {
      val c = AppConfig.create()
      c.close()
      i += 1
    }

    println(s"memtest scala OK (N=$n)")
  }
}
