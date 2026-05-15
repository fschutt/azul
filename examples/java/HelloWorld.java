// examples/java/HelloWorld.java — Python-quality Java port.
//
// Uses the typed `LayoutCallback` SAM that returns a `Dom` directly
// (CC-2) and the wrapper-class `App` API (CC-5): no JNA byte splice,
// no Marshal/AllocHGlobal-style pointer dance.
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

    // Typed layout callback: returns Dom directly. The host-invoker
    // bridge handles the AzDom-byte splice into the libazul out-pointer.
    private static final AzulHostInvoker.LayoutCallback LAYOUT =
        (long id, Pointer dataPtr, Pointer infoPtr) -> {
            Object recovered = AzulHostInvoker.refanyGet(dataPtr);
            if (!(recovered instanceof MyDataModel)) {
                return Dom.createBody();
            }
            MyDataModel m = (MyDataModel) recovered;
            Dom label = Dom.createDiv()
                .withCss("font-size: 32px;")
                .withChild(Dom.createText(String.valueOf(m.counter)));
            Dom buttonDom = Button.create("Increase counter")
                .withButtonType(AzButtonType.Primary.value)
                .onClick(m, ON_CLICK)
                .dom();
            return Dom.createBody()
                .withChild(label)
                .withChild(buttonDom);
        };

    public static void main(String[] args) {
        try (App app = App.create(AzulHostInvoker.refanyWrap(MODEL), AppConfig.create())) {
            app.run(WindowCreateOptions.create(LAYOUT));
        }
    }
}
