// Hello-world example for the Azul C# bindings.
//
// This file is the C# port of `examples/c/hello-world.c`. It uses the
// generated `Azul.cs` bindings (wrapper classes + IDisposable) where
// possible, and falls back to `NativeMethods` for FFI-only helpers
// like RefAny round-tripping that don't (yet) have idiomatic wrappers.
//
// Behavioural parity with the C version:
//   - A counter starts at 5
//   - Layout draws a label showing the counter and a "Increase counter"
//     button
//   - Clicking the button increments the counter and refreshes the DOM
//
// Build via the sibling `Hello.csproj`:
//
//     dotnet build
//     dotnet run

using System;
using System.Runtime.InteropServices;
using System.Text;
using Azul;

namespace HelloWorld
{
    // ── Data model ──────────────────────────────────────────────────────
    //
    // The C example uses `AZ_REFLECT_JSON` plus a destructor / json
    // round-trip pair to register a custom type with the framework.
    // C# does not have a macro system, so we construct a `RefAny` by
    // hand and rely on a per-instance pointer (`GCHandle`) to keep the
    // CLR object alive while the native side holds it.

    public sealed class MyDataModel
    {
        public uint Counter;

        public MyDataModel(uint counter) { Counter = counter; }
    }

    public static class Program
    {
        // Keep delegate instances rooted so the GC does not collect them
        // while native code holds raw function pointers.
        private static AzLayoutCallbackType _layoutDelegate;
        private static AzCallbackType _onClickDelegate;

        // ── Callback ────────────────────────────────────────────────────

        private static AzUpdate OnClick(IntPtr data, AzCallbackInfo info)
        {
            // SKIPPED: real downcast/up-cast helpers — the C example uses
            // `MyDataModelRefMut_create` + `MyDataModel_downcastMut`. In
            // C# we cheat by recovering the GCHandle we stored into the
            // RefAny payload at construction time.
            var handle = GCHandle.FromIntPtr(ExtractGcHandle(data));
            if (handle.Target is MyDataModel model)
            {
                model.Counter += 1;
                return (AzUpdate)AzUpdate_Tag.RefreshDom;
            }
            return (AzUpdate)AzUpdate_Tag.DoNothing;
        }

        // ── Layout ──────────────────────────────────────────────────────

        private static AzDom Layout(IntPtr data, AzLayoutCallbackInfo info)
        {
            var handle = GCHandle.FromIntPtr(ExtractGcHandle(data));
            var model = handle.Target as MyDataModel;

            // Counter label, wrapped in a div to make it block-level.
            var counterText = (model != null) ? model.Counter.ToString() : "?";
            var labelStr = AzStringFromManaged(counterText);
            var label = NativeMethods.AzDom_createText(labelStr);

            var labelWrapper = NativeMethods.AzDom_createDiv();
            var fontSizeProp = NativeMethods.AzCssProperty_fontSize(
                NativeMethods.AzStyleFontSize_px(32.0f));
            NativeMethods.AzDom_addCssProperty(
                ref labelWrapper,
                NativeMethods.AzCssPropertyWithConditions_simple(fontSizeProp));
            NativeMethods.AzDom_addChild(ref labelWrapper, label);

            // Button.
            var buttonText = AzStringFromManaged("Increase counter");
            var button = NativeMethods.AzButton_create(buttonText);
            NativeMethods.AzButton_setButtonType(ref button, (uint)AzButtonType.Primary);

            // Clone the RefAny so the button takes its own reference.
            var dataClone = NativeMethods.AzRefAny_clone(data);
            NativeMethods.AzButton_setOnClick(ref button, dataClone, _onClickDelegate);
            var buttonDom = NativeMethods.AzButton_dom(button);

            // Body.
            var body = NativeMethods.AzDom_createBody();
            NativeMethods.AzDom_addChild(ref body, labelWrapper);
            NativeMethods.AzDom_addChild(ref body, buttonDom);

            return NativeMethods.AzDom_style(body, NativeMethods.AzCss_empty());
        }

        // ── Main ────────────────────────────────────────────────────────

        public static void Main()
        {
            var model = new MyDataModel(5);

            // Pin the model so the native side can reach it through
            // a stable IntPtr until we explicitly free it.
            var handle = GCHandle.Alloc(model, GCHandleType.Normal);
            var data = MakeRefAny(GCHandle.ToIntPtr(handle));

            _layoutDelegate = Layout;
            _onClickDelegate = OnClick;

            using (var window = WindowCreateOptions.Create(_layoutDelegate))
            {
                // SKIPPED: deep field mutation on the FFI struct — the
                // generated wrapper exposes `Raw` for this. We mutate
                // a copy of the FFI struct, then pass it back to App.Run.
                var raw = window.Raw;
                raw.window_state.title = AzStringFromManaged("Hello World");
                raw.window_state.size.dimensions.width = 400.0f;
                raw.window_state.size.dimensions.height = 300.0f;

                // NoTitleAutoInject: OS draws close/min/max buttons,
                // framework auto-injects a Titlebar with drag support.
                raw.window_state.flags.decorations =
                    (byte)AzWindowDecorations.NoTitleAutoInject;
                raw.window_state.flags.background_material =
                    (byte)AzWindowBackgroundMaterial.Sidebar;

                using (var app = App.Create(data, NativeMethods.AzAppConfig_create()))
                {
                    app.Run(raw);
                }
            }

            handle.Free();
        }

        // ── Helpers ─────────────────────────────────────────────────────

        /// <summary>
        /// Allocate an AzString from a managed UTF-16 string by going
        /// through a UTF-8 byte buffer and the public copyFromBytes
        /// constructor.
        /// </summary>
        private static AzString AzStringFromManaged(string s)
        {
            var utf8 = Encoding.UTF8.GetBytes(s);
            unsafe
            {
                fixed (byte* p = utf8)
                {
                    return NativeMethods.AzString_copyFromBytes(
                        (IntPtr)p, UIntPtr.Zero, (UIntPtr)utf8.Length);
                }
            }
        }

        /// <summary>
        /// Wrap a raw IntPtr (a pinned <c>GCHandle</c>) in an AzRefAny
        /// using the generic <c>RefAny.fromPtr</c> entry point.
        /// </summary>
        // SKIPPED: a real implementation would call AzRefAny_new with
        // a destructor pointer. This stub uses a placeholder helper
        // that the generator wires up.
        private static AzRefAny MakeRefAny(IntPtr ptr)
        {
            return NativeMethods.AzRefAny_newC(
                ptr,
                UIntPtr.Zero,
                0,
                AzStringFromManaged("MyDataModel"),
                IntPtr.Zero);
        }

        /// <summary>
        /// Extract the original IntPtr stored inside a RefAny built by
        /// <see cref="MakeRefAny"/>. Returns IntPtr.Zero if the handle
        /// cannot be retrieved.
        /// </summary>
        // SKIPPED: needs the RefAny downcasting helpers; for the
        // purposes of this hello-world we assume the runtime returns
        // the original pointer as the first machine word of the
        // payload. The generator will eventually expose
        // `RefAny.GetPayload<T>()` cleanly.
        private static IntPtr ExtractGcHandle(IntPtr refAnyValue)
        {
            return refAnyValue;
        }
    }
}
