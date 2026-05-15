// examples/csharp/hello-world.cs — Python-quality C# port.
//
// Uses the wrapper-class `App.Create(...).Run(wco)` path (CC-5) and
// a typed layout delegate returning a wrapper `Dom` (CC-2). No
// Marshal.AllocHGlobal, no .Raw extraction, no IntPtr ceremony.
//
// Build + run (macOS):
//     DYLD_LIBRARY_PATH=. dotnet run

using System;
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

        private static Dom Layout(IntPtr dataPtr, IntPtr infoPtr)
        {
            var m = HostInvoker.RefanyGet(dataPtr) as MyDataModel;
            if (m == null) return Dom.CreateBody();
            var label = Dom.CreateDiv()
                .WithCss("font-size: 32px;")
                .WithChild(Dom.CreateText(m.Counter.ToString()));
            var buttonDom = Button.Create("Increase counter")
                .WithButtonType(AzButtonType.Primary)
                .OnClick(m, new Func<IntPtr, IntPtr, int>(OnClick))
                .Dom();
            return Dom.CreateBody()
                .WithChild(label)
                .WithChild(buttonDom);
        }

        public static int Main(string[] args)
        {
            using var app = App.Create(HostInvoker.RefanyWrap(_model), AppConfig.Create());
            app.Run(WindowCreateOptions.Create(new Func<IntPtr, IntPtr, Dom>(Layout)));
            return 0;
        }
    }
}
