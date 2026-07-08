// Memory test for the azul Kotlin (JNA, com.azul.*) binding. See tests/memtest/README.md.
//
// The harness (scripts/run_memtest.sh) does the measuring: it runs this twice
// with a small and a large AZ_MEMTEST_N and compares peak RSS (RSS that scales
// with N is a LEAK), and watches for any crash (double-free / UAF). This file
// only exercises the create/consume/DROP paths N times and exits 0. No event
// loop (App.run needs a display and hangs headless).
//
// Build/run like examples/kotlin/HelloWorld.kt (JNA, -Djna.library.path=.),
// but against this file instead of HelloWorld.kt (main class com.azul.MemTestKt).

package com.azul

class MyDataModel(var counter: Int)
private val MODEL = MyDataModel(5)

fun main() {
    val n = System.getenv("AZ_MEMTEST_N")?.trim()?.takeIf { it.isNotEmpty() }?.toInt() ?: 200000

    // 1. Consume-by-value DROP path once: App.create consumes AppConfig (whose
    //    nested SystemStyle was one of the bitwise-clone/double-free types).
    //    use{} closes the App instead of running an event loop.
    App.create(AzulHostInvoker.refanyWrap(MODEL), AppConfig.create()).use { _ ->
        // no run(): would need a display and hang headless.
    }

    // 2. Leak loop: create/close a droppable object N times. use{} (Closeable)
    //    frees the native data every iteration; we do NOT rely on GC/finalizer
    //    timing, which would mask a leak.
    for (i in 0 until n) {
        AppConfig.create().use { _ ->
            // closed at end of block
        }
    }

    println("memtest kotlin OK (N=$n)")
}
