// examples/csharp/hello-world.cs — Python-quality C# port.
//
// Uses the smart `WindowCreateOptions.Create(Layout)` factory; user
// code never has to set `wco.window_state.layout_callback` manually.
//
// Build + run (macOS):
//     DYLD_LIBRARY_PATH=. dotnet run

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
        private static readonly MyDataModel _model = new MyDataModel(5);

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
            if (m == null) return Dom.CreateBody().Raw;
            var label = Dom.CreateDiv()
                .WithCss("font-size: 32px;")
                .WithChild(Dom.CreateText(m.Counter.ToString()));
            var buttonDom = new Dom(
                Button.Create("Increase counter")
                    .WithButtonType(AzButtonType.Primary)
                    .OnClick(m, new Func<IntPtr, IntPtr, int>(OnClick))
                    .Dom());
            return Dom.CreateBody()
                .WithChild(label)
                .WithChild(buttonDom)
                .Raw;
        }

        public static int Main(string[] args)
        {
            // Smart factory: hides the host-invoker register +
            // window_state.layout_callback splice. Returns the wrapper
            // class; pull the raw struct back out for AzApp_run.
            using var wco = WindowCreateOptions.Create(new Func<IntPtr, IntPtr, AzDom>(Layout));
            var rawWco = wco.Raw;

            var appRaw = NativeMethods.AzApp_create(
                HostInvoker.RefanyCreate(_model), NativeMethods.AzAppConfig_create());

            var appPtr = Marshal.AllocHGlobal(Marshal.SizeOf<AzApp>());
            try
            {
                Marshal.StructureToPtr(appRaw, appPtr, false);
                NativeMethods.AzApp_run(appPtr, rawWco);
            }
            finally { Marshal.FreeHGlobal(appPtr); }
            return 0;
        }
    }
}
