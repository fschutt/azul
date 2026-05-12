// examples/java/HelloWorld.java
//
// Java port of examples/c/hello-world.c. Same data model (a counter),
// same behaviour (mouse click increments, layout rebuilds the DOM).
// Callbacks go through libazul's host-invoker plumbing — JNA never has
// to synthesize a struct-by-value trampoline for user code.
//
// Build + run (macOS):
//     mvn package
//     DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
//         -cp target/hello-world-1.0.0.jar:$HOME/.m2/repository/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar \
//         com.azul.HelloWorld
//
// Note: macOS requires `-XstartOnFirstThread` because libazul's event
// loop pumps NSApplication on the calling thread, which must be the
// process's main thread on Cocoa.

package com.azul;

import com.sun.jna.Pointer;

public final class HelloWorld {

    public static final class MyDataModel {
        public int counter;
        public MyDataModel(int counter) { this.counter = counter; }
    }

    private static final MyDataModel MODEL = new MyDataModel(5);

    // Build an AzString from a Java String. AzString_fromUtf8 takes its
    // own copy inside, so the JNA Memory buffer can be released after.
    private static AzString.ByValue str(java.lang.String s) {
        byte[] bytes = s.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        com.sun.jna.Memory mem = new com.sun.jna.Memory(bytes.length);
        mem.write(0, bytes, 0, bytes.length);
        return AzulNative.AzString_fromUtf8(mem, bytes.length);
    }

    // The AzulHostInvoker dispatch contract is: every registered
    // callback must be an instance of the matching <Kind>InvokerCallback
    // — a JNA Callback interface that takes raw native pointers. The
    // user is responsible for unboxing the refany, calling user code,
    // and writing the return value to outPtr via Structure.write.

    private static final AzulNativeManaged.CallbackInvokerCallback ON_CLICK_INVOKER =
        (long id, Pointer dataPtr, Pointer infoPtr, Pointer outPtr) -> {
            Object m = AzulHostInvoker.refanyGet(dataPtr);
            int result = 0; // AzUpdate.DoNothing
            if (m instanceof MyDataModel) {
                ((MyDataModel) m).counter += 1;
                result = 1; // AzUpdate.RefreshDom
            }
            outPtr.setInt(0, result);
        };

    private static final AzulNativeManaged.LayoutCallbackInvokerCallback LAYOUT_INVOKER =
        (long id, Pointer dataPtr, Pointer infoPtr, Pointer outPtr) -> {
            Object recovered = AzulHostInvoker.refanyGet(dataPtr);
            if (!(recovered instanceof MyDataModel)) {
                AzDom.ByValue empty = AzulNative.AzDom_createBody();
                empty.write();
                outPtr.write(0, empty.getPointer().getByteArray(0, empty.size()), 0, empty.size());
                return;
            }
            MyDataModel m = (MyDataModel) recovered;

            AzCallback.ByValue clickCb = AzulHostInvoker.registerCallback(ON_CLICK_INVOKER);
            AzRefAny.ByValue clickData = AzulHostInvoker.refanyCreate(m);

            // <div font-size:32px><text>{counter}</text></div>
            AzDom.ByValue counterText =
                AzulNative.AzDom_createText(str(java.lang.String.valueOf(m.counter)));
            AzDom.ByValue label =
                AzulNative.AzDom_withChild(
                    AzulNative.AzDom_withCss(
                        AzulNative.AzDom_createDiv(),
                        str("font-size: 32px;")
                    ),
                    counterText
                );

            // <button>Increase counter</button>
            AzButton.ByValue btn =
                AzulNative.AzButton_withOnClick(
                    AzulNative.AzButton_withButtonType(
                        AzulNative.AzButton_create(str("Increase counter")),
                        AzButtonType.Primary.value
                    ),
                    clickData,
                    clickCb
                );

            AzDom.ByValue body = AzulNative.AzDom_withChild(
                AzulNative.AzDom_withChild(
                    AzulNative.AzDom_createBody(),
                    label
                ),
                AzulNative.AzButton_dom(btn)
            );

            // Marshal the body bytes into the framework's out-pointer
            // so the static thunk can return our DOM. body.write() flushes
            // any pending field writes from the Java side; getByteArray
            // gives us the raw bytes JNA prepared.
            body.write();
            byte[] bytes = body.getPointer().getByteArray(0, body.size());
            outPtr.write(0, bytes, 0, bytes.length);
        };

    public static void main(java.lang.String[] args) {
        AzRefAny.ByValue data = AzulHostInvoker.refanyCreate(MODEL);
        AzLayoutCallback.ByValue layoutCb = AzulHostInvoker.registerLayoutCallback(LAYOUT_INVOKER);

        AzWindowCreateOptions.ByValue wco = AzulNative.AzWindowCreateOptions_default();
        // JNA assignment to a nested-struct field is a Java reference
        // swap, not a byte copy into the parent's storage. Flush layoutCb
        // bytes into the wco's existing layout_callback memory directly.
        layoutCb.write();
        wco.write();
        byte[] cbBytes = layoutCb.getPointer().getByteArray(0, layoutCb.size());
        wco.window_state.layout_callback.getPointer().write(0, cbBytes, 0, cbBytes.length);
        // Re-read so the in-Java mirror reflects the byte change.
        wco.read();

        AzAppConfig.ByValue cfg = AzulNative.AzAppConfig_create();
        AzApp.ByValue app = AzulNative.AzApp_create(data, cfg);
        app.write();
        AzulNative.AzApp_run(app.getPointer(), wco);
    }
}
