// Hello World: a counter and a button that increments it.
//
// Build (azul.swift, azul.h, module.modulemap and libazul next to this file):
//   swiftc -emit-library -emit-module -module-name Azul -parse-as-library -I. azul.swift -L. -lazul -o libAzulSwift.so
//   swiftc -I. hello-world.swift -L. -lAzulSwift -lazul -o hello-world
import Azul

// The application state is an ordinary Swift class. libazul keeps it alive
// and hands it back to every callback with its own type.
final class Counter {
    var count = 5
}

func layout(_ counter: Counter, _ info: LayoutCallbackInfo) -> Dom {
    let label = Dom.pWithText(String(counter.count))
        .withCss("font-size: 32px; margin: 0;")

    let button = Button("Increase counter")
        .withButtonType(.primary)
        .withOnClick(counter) { counter, _ in
            counter.count += 1
            return .refreshDom
        }

    return Dom.body()
        .withChild(label)
        .withChild(button.dom())
}

let window = WindowCreateOptions(layout)
window.windowState.title = "Hello World"
window.windowState.size.dimensions.width = 400
window.windowState.size.dimensions.height = 300

let app = App(Counter(), AppConfig())
app.run(window)
