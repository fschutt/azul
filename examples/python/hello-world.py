# python hello-world.py

from azul import *


# Plain Python class - "single source of truth" for app state
class DataModel:
    def __init__(self, counter):
        self.counter = counter


# Layout callback: f(DataModel, LayoutCallbackInfo) -> Dom. Runs once on
# startup and again after every callback that returns Update.RefreshDom.
def layout(data, info):
    # Rendered counter label: a text node wrapped in a styled div.
    # .with_css(...) consumes self and returns a new Dom, so builder
    # calls chain inline.
    label = (Dom.create_div()
             .with_child(Dom.create_text(str(data.counter)))
             .with_css("font-size: 32px;"))

    # Button widget with a click handler. with_on_click(data, callback)
    # registers the handler; .dom() turns the widget into a Dom node.
    button = (Button.create("Increase counter")
              .with_on_click(data, on_click)
              .dom()
              .with_css("flex-grow: 1;"))

    # Dom.create_body builds the root, .with_child(...) appends children.
    # Builder methods return a new Dom - keep chaining or re-assign.
    return (Dom.create_body()
            .with_child(label)
            .with_child(button))


# Click callback: f(DataModel, CallbackInfo) -> Update. 'data' is the same
# Python instance passed to App.create - mutate it in place. Update
# variants are plain class attributes: return Update.RefreshDom.
def on_click(data, info):
    data.counter += 1
    return Update.RefreshDom


if __name__ == "__main__":
    model = DataModel(5)
    window = WindowCreateOptions.create(layout)
    app = App.create(model, AppConfig.create())
    app.run(window)
