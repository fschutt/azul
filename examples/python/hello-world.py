from azul import *


class DataModel:
    def __init__(self, counter):
        self.counter = counter


def layout(data, info):
    label = (Dom.create_text(str(data.counter))
             .with_css("font-size:50px;"))

    button = (Dom.create_div()
              .with_css("flex-grow:1;")
              .with_child(Dom.create_text("Increase counter"))
              .with_callback(
                  EventFilter.Hover(HoverEventFilter.MouseUp()),
                  data,
                  on_click))

    body = (Dom.create_body()
            .with_child(label)
            .with_child(button))

    return body.style(Css.empty())


def on_click(data, info):
    data.counter += 1
    return Update.RefreshDom()


model = DataModel(5)
window = WindowCreateOptions.create(layout)

app = App.create(model, AppConfig.create())
app.run(window)
