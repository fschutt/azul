import Azul

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
