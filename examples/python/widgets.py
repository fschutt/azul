from azul import *


class WidgetShowcase:
    def __init__(self):
        self.enable_padding = True
        self.active_tab = 0
        self.progress_value = 25.0
        self.checkbox_checked = False
        self.text_input = ""


CLICK = EventFilter.Hover(HoverEventFilter.MouseUp)


def small(icon, label):
    return RibbonItem.SmallButton(RibbonButton.new(icon, label))


def menu(icon, label):
    return RibbonItem.SmallButton(RibbonButton.new(icon, label).with_arrow(RibbonArrow.Menu))


def large(icon, label, arrow):
    return RibbonItem.LargeButton(RibbonButton.new(icon, label).with_arrow(arrow))


def stack(items, container):
    for item in items:
        container = container.with_item(item)
    return container


def home_tab():
    clipboard = (RibbonGroup.new("Clipboard")
                 .with_item(large("content_paste", "Paste", RibbonArrow.Split))
                 .with_item(RibbonItem.Column(stack([
                     small("content_cut", "Cut"),
                     small("content_copy", "Copy"),
                     small("format_paint", "Format Painter"),
                 ], RibbonColumn.new()))))

    font_top = RibbonItem.Row(stack([
        small("text_increase", ""),
        small("text_decrease", ""),
        menu("text_fields", ""),
        small("format_clear", ""),
    ], RibbonRow.new()))
    font_bottom = RibbonItem.Row(stack([
        small("format_bold", ""),
        small("format_italic", ""),
        small("format_underlined", ""),
        small("strikethrough_s", ""),
        RibbonItem.Separator(),
        menu("format_color_text", ""),
    ], RibbonRow.new()))
    font = RibbonGroup.new("Font").with_item(
        RibbonItem.Column(stack([font_top, font_bottom], RibbonColumn.new())))

    para_top = RibbonItem.Row(stack([
        menu("format_list_bulleted", ""),
        menu("format_list_numbered", ""),
        RibbonItem.Separator(),
        small("format_indent_decrease", ""),
        small("format_indent_increase", ""),
    ], RibbonRow.new()))
    para_bottom = RibbonItem.Row(stack([
        small("format_align_left", ""),
        small("format_align_center", ""),
        small("format_align_right", ""),
        RibbonItem.Separator(),
        menu("format_line_spacing", ""),
    ], RibbonRow.new()))
    paragraph = RibbonGroup.new("Paragraph").with_item(
        RibbonItem.Column(stack([para_top, para_bottom], RibbonColumn.new())))

    editing = RibbonGroup.new("Editing").with_item(RibbonItem.Column(stack([
        menu("search", "Find"),
        small("find_replace", "Replace"),
        menu("highlight_alt", "Select"),
    ], RibbonColumn.new())))

    return (RibbonTab.new("HOME")
            .with_group(clipboard)
            .with_group(font)
            .with_group(paragraph)
            .with_group(editing))


def ribbon(data):
    return (Ribbon.new(RibbonTabVec.from_item(home_tab()))
            .with_app_button(RibbonAppButton.new("FILE"))
            .with_active_tab(data.active_tab)
            .dom())


def layout(data, info):
    button = (Dom.create_div()
              .with_css("margin-bottom:10px;padding:10px;background:#4CAF50;"
                        "color:white;cursor:pointer;")
              .with_child(Dom.create_text_do_not_use_without_block_level_wrapper("Click me!"))
              .with_callback(CLICK, data, on_button_click))

    checkbox = (CheckBox.create(data.checkbox_checked)
                .with_on_toggle(data, on_checkbox_toggle)
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

    content = (Dom.create_div()
               .with_css("flex-grow:1;overflow:auto;padding:20px;background:white;")
               .with_child(button)
               .with_child(checkbox)
               .with_child(progress)
               .with_child(text_input)
               .with_child(color_input)
               .with_child(number_input))

    return (Dom.create_body()
            .with_css("display:flex;flex-direction:column;height:100%;margin:0;padding:0;"
                      "font-family:sans-serif;")
            .with_child(ribbon(data))
            .with_child(content))


def on_button_click(data, info):
    data.progress_value += 10.0
    if data.progress_value > 100.0:
        data.progress_value = 0.0
    return Update.RefreshDom


def on_checkbox_toggle(data, info, state):
    data.checkbox_checked = state.checked
    return Update.RefreshDom


model = WidgetShowcase()
window = WindowCreateOptions.create(layout)
app = App.create(model, AppConfig.create())
app.run(window)
