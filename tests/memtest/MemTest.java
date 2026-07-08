// Memory test for the azul Java (JNA, com.azul.*) binding. See tests/memtest/README.md.
//
// The harness (scripts/run_memtest.sh) does the measuring: it runs this twice
// with a small and a large AZ_MEMTEST_N and compares peak RSS (RSS that scales
// with N is a LEAK), and watches for any crash (double-free / UAF). This file
// only exercises the create/consume/DROP paths N times and exits 0. No event
// loop (App.run needs a display and hangs headless).
//
// Build/run like examples/java/HelloWorld.java (JNA, -Djna.library.path=.),
// but against this class instead of HelloWorld.

package com.azul;

public final class MemTest {

    public static final class MyDataModel {
        public int counter;
        public MyDataModel(int counter) { this.counter = counter; }
    }

    private static final MyDataModel MODEL = new MyDataModel(5);

    public static void main(String[] args) {
        int n = 200000;
        String env = System.getenv("AZ_MEMTEST_N");
        if (env != null && !env.isEmpty()) n = Integer.parseInt(env.trim());

        // 1. Consume-by-value DROP path once: App.create consumes AppConfig
        //    (whose nested SystemStyle was one of the bitwise-clone/double-free
        //    types). close() the App instead of running an event loop.
        try (App app = App.create(AzulHostInvoker.refanyWrap(MODEL), AppConfig.create())) {
            // no run(): would need a display and hang headless.
        }

        // 2. Leak loop: create/close a droppable object N times. AutoCloseable
        //    close() frees the native data every iteration; we do NOT rely on
        //    GC/finalizer timing, which would mask a leak.
        for (int i = 0; i < n; i++) {
            try (AppConfig c = AppConfig.create()) {
                // closed at end of block
            }
        }

        System.out.println("memtest java OK (N=" + n + ")");
    }
}
