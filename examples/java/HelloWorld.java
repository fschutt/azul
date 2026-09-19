package com.azul;

class Counter {
    public int count = 5;
}

public class HelloWorld {
    public static void main(String[] args) {
        try (App app = App.create(new Counter(), HelloWorld::layout)) {
            WindowCreateOptions options = WindowCreateOptions.create();
            app.run(options);
        }
    }

    public static Dom layout(Counter data, LayoutCallbackInfo info) {
        String countStr = String.format("%d", data.count);
        Button btn = Button.create("Increase counter")
            .withOnClick(data, HelloWorld::onClick);
        
        return Dom.createBody()
            .withChild(Dom.createPWithText(countStr))
            .withChild(btn.dom());
    }

    public static Update onClick(Counter data, CallbackInfo info) {
        data.count++;
        return Update.RefreshDom; 
    }
}
