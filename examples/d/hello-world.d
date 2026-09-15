module hello_world;

import azul;
import std.conv : to;

final class Counter
{
    int count = 5;
}

Dom layout(Counter counter, LayoutCallbackInfo info)
{
    auto label = Dom.pWithText(counter.count.to!string)
        .withCss("font-size: 32px; margin: 0;");

    auto button = Button("Increase counter")
        .withButtonType(ButtonType.primary)
        .withOnClick(counter, (Counter c, CallbackInfo _) {
            c.count += 1;
            return Update.refreshDom;
        });

    return Dom.body()
        .withChild(label)
        .withChild(button.dom());
}

void main()
{
    auto window = WindowCreateOptions(&layout);
    window.windowState.title = "Hello World";
    window.windowState.size.dimensions.width = 400;
    window.windowState.size.dimensions.height = 300;

    auto app = App(new Counter, AppConfig());
    app.run(window);
}
