// examples/csharp/hello-world.cs
//
// Minimal C# smoke test for the Azul host-invoker plumbing. Confirms
// that the embedded C# compiles, the dylib loads, and the host-invoker
// init phase (registerCallback / refanyCreate) succeeds.
//
// Full GUI wiring (Dom builders, WindowCreateOptions, App.Run) requires
// the wrapper layer's idiomatic API surface to settle — separate work.
//
// Build + run:
//     dotnet run

using System;
using System.Runtime.InteropServices;
using Azul;

namespace HelloWorld
{
    public sealed class MyDataModel
    {
        public uint Counter;
        public MyDataModel(uint counter) { Counter = counter; }
    }

    public static class Program
    {
        public static int Main(string[] args)
        {
            // Wrap a managed object in an AzRefAny via the host-handle path.
            var model = new MyDataModel(5);
            var data  = HostInvoker.RefanyCreate(model);
            Console.WriteLine("[azul] refanyCreate ran; RefAny opaque-handle id stored.");

            // Recover the same object through refanyGet.
            // (RefAny is a struct-by-value FFI type; we marshal it through
            // an IntPtr via the same shape the framework's callbacks see.)
            var ptr = Marshal.AllocHGlobal(Marshal.SizeOf<AzRefAny>());
            try
            {
                Marshal.StructureToPtr(data, ptr, false);
                var recovered = HostInvoker.RefanyGet(ptr);
                if (recovered is MyDataModel m && m.Counter == 5)
                {
                    Console.WriteLine("[azul] refanyGet round-trip succeeded; counter=" + m.Counter);
                }
                else
                {
                    Console.WriteLine("[azul] refanyGet round-trip FAILED (recovered=" + recovered + ")");
                    return 1;
                }
            }
            finally
            {
                Marshal.FreeHGlobal(ptr);
            }

            Console.WriteLine("[azul] host-invoker init phase completed successfully.");
            Console.WriteLine("[azul] (Full App.Run wiring requires wrapper-layer API surface");
            Console.WriteLine("[azul]  fixes that are separate from the host-invoker plumbing.)");
            return 0;
        }
    }
}
