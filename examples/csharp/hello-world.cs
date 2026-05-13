// examples/csharp/hello-world.cs
//
// C# port of examples/c/hello-world.c. Same data model (a counter),
// same behaviour (mouse click increments, layout rebuilds the DOM).
// Callbacks go through libazul's host-invoker plumbing — P/Invoke
// never has to synthesize a struct-by-value trampoline for user code.
//
// Build + run (macOS):
//     DYLD_LIBRARY_PATH=. dotnet run

using System;
using System.Runtime.InteropServices;
using System.Text;
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
        // Module-level so callbacks can reach it through the refany handle.
        private static MyDataModel _model = new MyDataModel(5);

        // Copy a managed C# string into an AzString. The bytes live in
        // an unmanaged buffer that AzString_fromUtf8 reads from; libazul
        // takes its own copy, so we can free the buffer when done.
        private static AzString Str(string s)
        {
            var bytes = Encoding.UTF8.GetBytes(s);
            var p = Marshal.AllocHGlobal(bytes.Length);
            try
            {
                Marshal.Copy(bytes, 0, p, bytes.Length);
                return NativeMethods.AzString_fromUtf8(p, (UIntPtr)bytes.Length);
            }
            finally
            {
                Marshal.FreeHGlobal(p);
            }
        }

        private static int OnClick(IntPtr dataPtr, IntPtr infoPtr)
        {
            var m = HostInvoker.RefanyGet(dataPtr) as MyDataModel;
            if (m == null) return (int)AzUpdate.DoNothing;
            m.Counter += 1;
            return (int)AzUpdate.RefreshDom;
        }

        private static AzDom Layout(IntPtr dataPtr, IntPtr infoPtr)
        {
            var m = HostInvoker.RefanyGet(dataPtr) as MyDataModel;
            if (m == null) return NativeMethods.AzDom_createBody();

            var clickCb = HostInvoker.RegisterCallback(new Func<IntPtr, IntPtr, int>(OnClick));
            var clickData = HostInvoker.RefanyCreate(m);

            // <div font-size:32px><text>{counter}</text></div>
            var counterText = NativeMethods.AzDom_createText(Str(m.Counter.ToString()));
            var label = NativeMethods.AzDom_withChild(
                NativeMethods.AzDom_withCss(
                    NativeMethods.AzDom_createDiv(),
                    Str("font-size: 32px;")
                ),
                counterText
            );

            // <button>Increase counter</button>
            var btn = NativeMethods.AzButton_withOnClick(
                NativeMethods.AzButton_withButtonType(
                    NativeMethods.AzButton_create(Str("Increase counter")),
                    AzButtonType.Primary
                ),
                clickData,
                clickCb
            );

            return NativeMethods.AzDom_withChild(
                NativeMethods.AzDom_withChild(
                    NativeMethods.AzDom_createBody(),
                    label
                ),
                NativeMethods.AzButton_dom(btn)
            );
        }

        public static int Main(string[] args)
        {
            var data = HostInvoker.RefanyCreate(_model);

            // Register the layout callback via host-invoker so the ctx
            // (carrying our dispatch handle) survives. The raw
            // AzWindowCreateOptions_create(AzLayoutCallbackType) constructor
            // takes a bare fn pointer and discards the ctx — use _default
            // + window_state.layout_callback assignment instead.
            var layoutCb = HostInvoker.RegisterLayoutCallback(
                new Func<IntPtr, IntPtr, AzDom>(Layout));

            var wco = NativeMethods.AzWindowCreateOptions_default();
            wco.window_state.layout_callback = layoutCb;

            // Don't wrap AppConfig in the IDisposable AppConfig class —
            // App.Create moves the struct into libazul by value, and the
            // wrapper's Dispose would later try to delete heap pointers
            // libazul has already taken ownership of (double-free).
            var appConfigRaw = NativeMethods.AzAppConfig_create();
            var appRaw = NativeMethods.AzApp_create(data, appConfigRaw);

            // app is passed by value to Run, then libazul owns it.
            var appPtr = Marshal.AllocHGlobal(Marshal.SizeOf<AzApp>());
            try
            {
                Marshal.StructureToPtr(appRaw, appPtr, false);
                NativeMethods.AzApp_run(appPtr, wco);
            }
            finally
            {
                Marshal.FreeHGlobal(appPtr);
            }
            return 0;
        }
    }
}
