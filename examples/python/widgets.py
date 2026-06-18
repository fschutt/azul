# python widgets.py

from azul import *


class WidgetShowcase:
    def __init__(self):
        self.enable_padding = True
        self.active_tab = 0
        self.progress_value = 25.0
        self.checkbox_checked = False
        self.text_input = ""


CLICK = EventFilter.Hover(HoverEventFilter.MouseUp())


def layout(data, info):
    title = (Dom.create_text("Widget Showcase")
             .with_css("font-size:24px;margin-bottom:20px;"))

    button = (Dom.create_div()
              .with_css("margin-bottom:10px;padding:10px;background:#4CAF50;"
                        "color:white;cursor:pointer;")
              .with_child(Dom.create_text("Click me!"))
              .with_callback(CLICK, data, on_button_click))

    checkbox = (CheckBox.create(data.checkbox_checked)
                .dom()
                .with_css("margin-bottom:10px;"))

    progress = (ProgressBar.create(data.progress_value)
                .dom()
                .with_css("margin-bottom:10px;"))

    text_input = (TextInput.create()
                  .with_placeholder("Enter text here...")
                  .dom()
                  .with_css("margin-bottom:10px;"))

    color_input = (ColorInput.create(ColorU(100, 150, 200, 255))
                   .dom()
                   .with_css("margin-bottom:10px;"))

    number_input = (NumberInput.create(42.0)
                    .dom()
                    .with_css("margin-bottom:10px;"))

    body = (Dom.create_body()
            .with_css("padding:20px;font-family:sans-serif;")
            .with_child(title)
            .with_child(button)
            .with_child(checkbox)
            .with_child(progress)
            .with_child(text_input)
            .with_child(color_input)
            .with_child(number_input))

    return body.style(Css.empty())


def on_button_click(data, info):
    data.progress_value += 10.0
    if data.progress_value > 100.0:
        data.progress_value = 0.0
    return Update.RefreshDom()


model = WidgetShowcase()
window = WindowCreateOptions.create(layout)
app = App.create(model, AppConfig.create())
app.run(window)
