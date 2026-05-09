// examples/csharp/hello-world.cs
//
// C# port of examples/c/hello-world.c built against the host-invoker
// runtime helpers in `Azul.cs` (see `lang_csharp/managed.rs`).
//
// Same shape as examples/lua/hello-world.lua and examples/perl/hello-world.pl:
//   * `Azul.HostInvoker.RefanyCreate(value)` wraps any managed object in
//     an AzRefAny held alive by the framework's refcount.
//   * Callbacks are plain C# delegates handed to
//     `Azul.HostInvoker.RegisterCallback(delegate)`, which returns the
//     `AzCallback` cdata struct the C ABI expects. The framework's static
//     thunk in libazul dispatches back into managed code via the libffi
//     closure registered at module load.
//
// Build + run:
//
//     dotnet build
//     dotnet run

using System;
using System.Runtime.InteropServices;
using Azul;

namespace HelloWorld
{
    // ── Data model ──────────────────────────────────────────────────────
    public sealed class MyDataModel
    {
        public uint Counter;
        public MyDataModel(uint counter) { Counter = counter; }
    }

    // Per-kind delegate signatures the host-invoker plumbing expects.
    // Argument list mirrors the framework's CallbackType: each by-value
    // aggregate (`AzRefAny`, `AzCallbackInfo`, `AzLayoutCallbackInfo`)
    // is passed by IntPtr — the host-invoker's pointer-arg convention.
    public delegate AzUpdate ClickHandler(IntPtr dataPtr, IntPtr infoPtr);
    public delegate AzDom    LayoutHandler(IntPtr dataPtr, IntPtr infoPtr);

    public static class Program
    {
        public static int Main(string[] args)
        {
            // ── Wrap the model in an AzRefAny ────────────────────────────
            var model = new MyDataModel(5);
            AzRefAny data = HostInvoker.RefanyCreate(model);

            // ── Callbacks ────────────────────────────────────────────────
            ClickHandler onClick = (IntPtr dataPtr, IntPtr infoPtr) =>
            {
                var m = HostInvoker.RefanyGet(dataPtr) as MyDataModel;
                if (m == null) return AzUpdate.DoNothing;
                m.Counter++;
                return AzUpdate.RefreshDom;
            };

            LayoutHandler layout = (IntPtr dataPtr, IntPtr infoPtr) =>
            {
                var m = HostInvoker.RefanyGet(dataPtr) as MyDataModel;
                if (m == null) return Dom.CreateBody().__inner;

                // Until lang_csharp/wrappers.rs learns to substitute
                // callback args via HostInvoker.Register*Callback
                // automatically, we wire it explicitly here. The CB
                // wrapper carries the host-handle ctx we need so the
                // static thunk in libazul can dispatch back to onClick.
                AzCallback clickCb = HostInvoker.RegisterCallback(onClick);

                var label        = Dom.CreateText(AzString.FromString(m.Counter.ToString()));
                var labelWrapper = Dom.CreateDiv();
                labelWrapper.AddCssProperty(
                    CssPropertyWithConditions.Simple(
                        CssProperty.FontSize(StyleFontSize.Px(32.0f))));
                labelWrapper.AddChild(label);

                var button = Button.Create(AzString.FromString("Increase counter"));
                button.SetButtonType(AzButtonType.Primary);
                AzRefAny dataClone = NativeMethods.AzRefAny_clone(data.__inner);
                button.SetOnClick(dataClone, clickCb);

                var body = Dom.CreateBody();
                body.AddChild(labelWrapper);
                body.AddChild(button.Dom().__inner);
                return body.__inner;
            };

            // ── Main ─────────────────────────────────────────────────────
            AzLayoutCallback layoutCb = HostInvoker.RegisterLayoutCallback(layout);
            // WindowCreateOptions.Create takes a raw LayoutCallbackType —
            // bypass via _default and direct field assignment so the
            // host-handle ctx survives, mirroring lang_lua's emitter fix.
            var window = WindowCreateOptions.Default();
            window.__inner.window_state.layout_callback = layoutCb;
            window.__inner.window_state.title = AzString.FromString("Hello World").__inner;
            window.__inner.window_state.size.dimensions.width  = 400.0f;
            window.__inner.window_state.size.dimensions.height = 300.0f;
            window.__inner.window_state.flags.decorations         = AzWindowDecorations.NoTitleAutoInject;
            window.__inner.window_state.flags.background_material = AzWindowBackgroundMaterial.Sidebar;

            using var app = App.Create(data, AppConfig.Create());
            app.Run(window);
            return 0;
        }
    }
}
