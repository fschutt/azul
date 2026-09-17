package com.azul;

class Counter {
    public int count = 0;
}

public class HelloWorld {
    public static void main(String[] args) {
        try (App app = App.create(new Counter(), HelloWorld::layout)) {
            WindowCreateOptions options = WindowCreateOptions.create();
            app.run(options);
        }
    }

    public static Dom layout(Counter data, AzLayoutCallbackInfo info) {
        String countStr = String.format("Count: %d", data.count);
        Button btn = Button.create(countStr)
            .withOnClick(data, HelloWorld::onClick);
        
        return Dom.createBody()
            .withChild(btn.dom());
    }

    public static Update onClick(Counter data, AzCallbackInfo info) {
        data.count++;
        return Update.RefreshDom; 
    }
}
