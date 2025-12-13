from azul import *

class DataModel:
    def __init__(self, counter):
        self.counter = counter

def layout(data, info):
    label = Dom.text(str(data.counter))
    label.set_inline_style("font-size:50px;")

    button = Dom.div()
    button.set_inline_style("flex-grow:1;")
    button.add_child(Dom.text("Increase counter"))
    button.set_callback(On.MouseUp, data, on_click)

    body = Dom.body()
    body.add_child(label)
    body.add_child(button)

    return body.style(Css.empty())

def on_click(data, info):
    data.counter += 1
    return Update.RefreshDom

model = DataModel(5)
window = WindowCreateOptions(layout)
window.set_title("Hello World")
window.set_dimensions(400, 300)

app = App(model, AppConfig.default())
app.run(window)