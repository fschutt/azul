// Memory test for the azul C# (P/Invoke) binding. See tests/memtest/README.md.
//
// The harness (scripts/run_memtest.sh) does the measuring: it runs this twice
// with a small and a large AZ_MEMTEST_N and compares peak RSS (RSS that scales
// with N is a LEAK), and watches for any crash (double-free / UAF). This file
// only exercises the create/consume/DROP paths N times and exits 0. No event
// loop (App.Run needs a display and hangs headless).
//
// Build/run like examples/csharp/hello-world.cs (dotnet run -c Release), but
// against this file instead of hello-world.cs.

using System;
using Azul;

namespace MemTest
{
    public sealed class MyDataModel
    {
        public uint Counter;
        public MyDataModel(uint counter) { Counter = counter; }
    }

    public static class Program
    {
        private static readonly MyDataModel _model = new MyDataModel(5);

        public static int Main(string[] args)
        {
            int n = 200000;
            var env = Environment.GetEnvironmentVariable("AZ_MEMTEST_N");
            if (!string.IsNullOrEmpty(env)) n = int.Parse(env);

            // 1. Consume-by-value DROP path once: App.Create consumes AppConfig
            //    (whose nested SystemStyle was one of the bitwise-clone/double-free
            //    types). Dispose the App instead of running an event loop.
            using (var app = App.Create(HostInvoker.RefanyWrap(_model), AppConfig.Create()))
            {
                // no Run(): would need a display and hang headless.
            }

            // 2. Leak loop: create/dispose a droppable object N times. `using`
            //    (IDisposable) frees the native data every iteration; we do NOT
            //    rely on GC/finalizer timing, which would mask a leak.
            for (int i = 0; i < n; i++)
            {
                using (var c = AppConfig.Create())
                {
                    // disposed at end of block
                }
            }

            Console.WriteLine($"memtest csharp OK (N={n})");
            return 0;
        }
    }
}
