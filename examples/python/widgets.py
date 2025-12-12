# Widgets Showcase - Python
# python widgets.py

from azul import *

class WidgetShowcase:
    def __init__(self):
        self.enable_padding = True
        self.active_tab = 0
        self.progress_value = 25.0
        self.checkbox_checked = False
        self.text_input = ""

def layout(data, info):
    # Create button
    button = Dom.div()
    button.set_inline_style("margin-bottom: 10px; padding: 10px; background: #4CAF50; color: white; cursor: pointer;")
    button.add_child(Dom.text("Click me!"))
    button.set_callback(On.MouseUp, data, on_button_click)

    # Create checkbox
    checkbox = CheckBox(data.checkbox_checked).dom()
    checkbox.set_inline_style("margin-bottom: 10px;")

    # Create progress bar
    progress = ProgressBar(data.progress_value).dom()
    progress.set_inline_style("margin-bottom: 10px;")

    # Create text input
    text_input = TextInput()
    text_input.set_placeholder("Enter text here...")
    text_input_dom = text_input.dom()
    text_input_dom.set_inline_style("margin-bottom: 10px;")

    # Create color input
    color = ColorU(100, 150, 200, 255)
    color_input = ColorInput(color).dom()
    color_input.set_inline_style("margin-bottom: 10px;")

    # Create number input
    number_input = NumberInput(42.0).dom()
    number_input.set_inline_style("margin-bottom: 10px;")

    # Create dropdown
    dropdown = DropDown(["Option 1", "Option 2", "Option 3"]).dom()
    dropdown.set_inline_style("margin-bottom: 10px;")

    # Compose body
    body = Dom.body()
    body.set_inline_style("padding: 20px; font-family: sans-serif;")
    
    title = Dom.text("Widget Showcase")
    title.set_inline_style("font-size: 24px; margin-bottom: 20px;")
    body.add_child(title)
    
    body.add_child(button)
    body.add_child(checkbox)
    body.add_child(progress)
    body.add_child(text_input_dom)
    body.add_child(color_input)
    body.add_child(number_input)
    body.add_child(dropdown)

    return body.style(Css.empty())

def on_button_click(data, info):
    data.progress_value += 10.0
    if data.progress_value > 100.0:
        data.progress_value = 0.0
    return Update.RefreshDom

model = WidgetShowcase()
window = WindowCreateOptions(layout)
window.set_title("Widget Showcase")
window.set_dimensions(600, 500)

app = App(model, AppConfig.default())
app.run(window)
