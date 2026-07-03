// mvn package && java -Djna.library.path=. -cp target/hello-world-1.0.0.jar:$HOME/.m2/repository/net/java/dev/jna/jna/5.14.0/jna-5.14.0.jar com.azul.HelloWorld

package com.azul;

import com.sun.jna.Pointer;

public final class HelloWorld {

    public static final class MyDataModel {
        public int counter;
        public MyDataModel(int counter) { this.counter = counter; }
    }

    private static final MyDataModel MODEL = new MyDataModel(5);

    private static final AzulNativeManaged.ButtonOnClickCallbackInvokerCallback ON_CLICK =
        (long id, Pointer dataPtr, Pointer infoPtr, Pointer outPtr) -> {
            Object m = AzulHostInvoker.refanyGet(dataPtr);
            int result = AzUpdate.DoNothing.value;
            if (m instanceof MyDataModel) {
                ((MyDataModel) m).counter += 1;
                result = AzUpdate.RefreshDom.value;
            }
            outPtr.setInt(0, result);
        };

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
