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

        private static Update OnClick(MyDataModel m, IntPtr info)
        {
            m.Counter += 1;
            return Update.RefreshDom;
        }

        private static Dom Layout(MyDataModel m, IntPtr info)
        {
            var label = Dom.CreatePWithText(m.Counter.ToString())
                .WithCss("font-size: 32px; margin: 0;");
            var buttonDom = Button.Create("Increase counter")
                .WithButtonType(ButtonType.Primary)
                .OnClick(m, OnClick)
                .Dom();
            return Dom.CreateBody()
                .WithChild(label)
                .WithChild(buttonDom);
        }

        public static int Main(string[] args)
        {
            using var app = App.Create(_model, AppConfig.Create());
            app.Run(WindowCreateOptions.Create<MyDataModel>(Layout));
            return 0;
        }
    }
}
