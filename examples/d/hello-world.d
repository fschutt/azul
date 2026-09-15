// Hello World: a counter and a button that increments it.
//
// Build (azul.d and libazul next to this file):
//   dmd hello-world.d azul.d -L-L. -L-lazul
//
// The file name has a hyphen, which is not a D identifier, so the module
// needs a name of its own.
module hello_world;

import azul;
import std.conv : to;

// The application state is an ordinary D class. libazul keeps it alive and
// hands it back to every callback with its own type.
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
