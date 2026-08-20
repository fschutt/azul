from azul import *


class DataModel:
    def __init__(self, counter):
        self.counter = counter


def layout(data, info):
    label = (Dom.create_div()
             .with_child(Dom.create_text_do_not_use_without_block_level_wrapper(str(data.counter)))
             .with_css("font-size: 32px;"))

    button = (Button.create("Increase counter")
              .with_on_click(data, on_click)
              .dom()
              .with_css("flex-grow: 1;"))

    return (Dom.create_body()
            .with_child(label)
            .with_child(button))


def on_click(data, info):
    data.counter += 1
    return Update.RefreshDom


if __name__ == "__main__":
    model = DataModel(5)
    window = WindowCreateOptions.create(layout)
    app = App.create(model, AppConfig.create())
    app.run(window)
