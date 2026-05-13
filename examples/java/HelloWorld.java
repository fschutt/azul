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
                Dom empty = Dom.createBody();
                AzDom.ByValue emptyRaw = (AzDom.ByValue) Structure.newInstance(AzDom.ByValue.class, empty.rawPointer());
                emptyRaw.read();
                outPtr.write(0, emptyRaw.getPointer().getByteArray(0, emptyRaw.size()), 0, emptyRaw.size());
                return;
            }
            MyDataModel m = (MyDataModel) recovered;
            Dom label = Dom.createDiv()
                .withCss("font-size: 32px;")
                .withChild(Dom.createText(java.lang.String.valueOf(m.counter)));
            Dom buttonDom = new Dom(
                Button.create("Increase counter")
                    .withButtonType(AzButtonType.Primary.value)
                    .onClick(m, ON_CLICK)
                    .dom()
                    .getPointer());
            Dom body = Dom.createBody()
                .withChild(label)
                .withChild(buttonDom);
            AzDom.ByValue bodyRaw = (AzDom.ByValue) Structure.newInstance(AzDom.ByValue.class, body.rawPointer());
            bodyRaw.read();
            outPtr.write(0, bodyRaw.getPointer().getByteArray(0, bodyRaw.size()), 0, bodyRaw.size());
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
