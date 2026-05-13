// examples/java/HelloWorld.java — Python-quality Java port.
//
// Uses the smart `WindowCreateOptions.create(LAYOUT)` factory that hides
// the host-invoker plumbing. User code never has to splice bytes via
// JNA `Pointer.write` or manage the AzLayoutCallback ↔ wco
// `window_state.layout_callback` byte-copy dance — the codegen does
// it inside the factory.
//
// Build + run (macOS):
//     mvn package
//     DYLD_LIBRARY_PATH=. java -XstartOnFirstThread -Djna.library.path=. \
//         -cp target/hello-world-1.0.0.jar:$HOME/.m2/repository/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar \
//         com.azul.HelloWorld
//
// macOS requires `-XstartOnFirstThread` so libazul's NSApplication
// loop pumps on the JVM main thread.

package com.azul;

import com.sun.jna.Pointer;
import com.sun.jna.Structure;

public final class HelloWorld {

    public static final class MyDataModel {
        public int counter;
        public MyDataModel(int counter) { this.counter = counter; }
    }

    private static final MyDataModel MODEL = new MyDataModel(5);

    private static AzString.ByValue str(java.lang.String s) {
        byte[] bytes = s.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        com.sun.jna.Memory mem = new com.sun.jna.Memory(bytes.length);
        mem.write(0, bytes, 0, bytes.length);
        return AzulNativeStr.AzString_fromUtf8(mem, bytes.length);
    }

    private static final AzulNativeManaged.CallbackInvokerCallback ON_CLICK =
        (long id, Pointer dataPtr, Pointer infoPtr, Pointer outPtr) -> {
            Object m = AzulHostInvoker.refanyGet(dataPtr);
            int result = AzUpdate.DoNothing.value;
            if (m instanceof MyDataModel) {
                ((MyDataModel) m).counter += 1;
                result = AzUpdate.RefreshDom.value;
            }
            outPtr.setInt(0, result);
        };

    private static final AzulNativeManaged.LayoutCallbackInvokerCallback LAYOUT =
        (long id, Pointer dataPtr, Pointer infoPtr, Pointer outPtr) -> {
            Object recovered = AzulHostInvoker.refanyGet(dataPtr);
            if (!(recovered instanceof MyDataModel)) {
                AzDom.ByValue empty = AzulNativeDom.AzDom_createBody();
                empty.write();
                outPtr.write(0, empty.getPointer().getByteArray(0, empty.size()), 0, empty.size());
                return;
            }
            MyDataModel m = (MyDataModel) recovered;
            AzDom.ByValue label = AzulNativeDom.AzDom_withChild(
                AzulNativeDom.AzDom_withCss(AzulNativeDom.AzDom_createDiv(), str("font-size: 32px;")),
                AzulNativeDom.AzDom_createText(str(java.lang.String.valueOf(m.counter))));
            AzButton.ByValue btn = AzulNativeWidgets.AzButton_withOnClick(
                AzulNativeWidgets.AzButton_withButtonType(
                    AzulNativeWidgets.AzButton_create(str("Increase counter")), AzButtonType.Primary.value),
                AzulHostInvoker.refanyCreate(m), AzulHostInvoker.registerCallback(ON_CLICK));
            AzDom.ByValue body = AzulNativeDom.AzDom_withChild(
                AzulNativeDom.AzDom_withChild(AzulNativeDom.AzDom_createBody(), label),
                AzulNativeWidgets.AzButton_dom(btn));
            body.write();
            outPtr.write(0, body.getPointer().getByteArray(0, body.size()), 0, body.size());
        };

    public static void main(java.lang.String[] args) {
        // Smart factory: hides the host-invoker registration + bytes-splice
        // that every JVM hello-world had to perform manually before.
        WindowCreateOptions wco = WindowCreateOptions.create(LAYOUT);
        AzWindowCreateOptions.ByValue rawWco =
            Structure.newInstance(AzWindowCreateOptions.ByValue.class, wco.rawPointer());
        rawWco.read();
        AzApp.ByValue app = AzulNativeApp.AzApp_create(
            AzulHostInvoker.refanyCreate(MODEL), AzulNativeApp.AzAppConfig_create());
        app.write();
        AzulNativeApp.AzApp_run(app.getPointer(), rawWco);
    }
}
