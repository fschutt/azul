// examples/csharp/hello-world.cs — Python-quality C# port.
//
// Uses the smart `WindowCreateOptions.Create(Layout)` factory; user
// code never has to set `wco.window_state.layout_callback` manually.
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
        private static readonly MyDataModel _model = new MyDataModel(5);

        private static AzString Str(string s)
        {
            var bytes = Encoding.UTF8.GetBytes(s);
            var p = Marshal.AllocHGlobal(bytes.Length);
            try
            {
                Marshal.Copy(bytes, 0, p, bytes.Length);
                return NativeMethods.AzString_fromUtf8(p, (UIntPtr)bytes.Length);
            }
            finally { Marshal.FreeHGlobal(p); }
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
            var label = NativeMethods.AzDom_withChild(
                NativeMethods.AzDom_withCss(NativeMethods.AzDom_createDiv(), Str("font-size: 32px;")),
                NativeMethods.AzDom_createText(Str(m.Counter.ToString())));
            var btn = NativeMethods.AzButton_withOnClick(
                NativeMethods.AzButton_withButtonType(
                    NativeMethods.AzButton_create(Str("Increase counter")), AzButtonType.Primary),
                HostInvoker.RefanyCreate(m),
                HostInvoker.RegisterCallback(new Func<IntPtr, IntPtr, int>(OnClick)));
            return NativeMethods.AzDom_withChild(
                NativeMethods.AzDom_withChild(NativeMethods.AzDom_createBody(), label),
                NativeMethods.AzButton_dom(btn));
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
