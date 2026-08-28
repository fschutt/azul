use azul::css::ColorU;
use azul::prelude::*;
use azul::widgets::*;

#[derive(Default, Clone)]
struct WidgetShowcase {
    enable_padding: bool,
    progress: f32,
    selected_choice: usize,
    active_tab: usize,
}

const CHOICES: &[&str] = &["Red", "Green", "Blue"];

extern "C" fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let showcase = match data.downcast_ref::<WidgetShowcase>() {
        Some(s) => (*s).clone(),
        None => return Dom::create_body(),
    };

    let enable_padding = showcase.enable_padding;
    let progress = showcase.progress;

    let toggle_text = if enable_padding {
        "Disable padding"
    } else {
        "Enable padding"
    };

    let padding = if enable_padding { "padding: 10px;" } else { "" };
    let margin = "margin-bottom: 10px;";

    let mut button = Button::create(toggle_text);
    button.set_on_click(data.clone(), toggle_padding);
    let button_dom = button.dom().with_css(margin);

    let mut checkbox = CheckBox::create(enable_padding);
    checkbox.set_on_toggle(data.clone(), toggle_padding_checkbox);
    let checkbox_dom = checkbox.dom().with_css("margin-bottom: 10px;");

    let progress_bar = ProgressBar::create(progress)
        .dom()
        .with_css("margin-bottom: 10px; width: 200px;");

    let text_input = TextInput::create()
        .with_placeholder("Type something...")
        .dom()
        .with_css(margin);

    let number_input = NumberInput::create(42.0).dom().with_css(margin);

    let color_input = ColorInput::create(ColorU::from_str("#FF5733"))
        .dom()
        .with_css(margin);

    let mut increase_button = Button::create("Increase Progress");
    increase_button.set_on_click(data.clone(), increase_progress);
    let increase_dom = increase_button.dom().with_css(margin);

    let choices: Vec<azul::str::String> = CHOICES.iter().map(|s| (*s).into()).collect();
    let dropdown_dom = DropDown::create(choices)
        .with_on_choice_change(data.clone(), on_dropdown_change)
        .dom()
        .with_css(margin);
    let selected_label = Dom::create_div_with_text(
        format!(
            "Selected: {}",
            CHOICES[showcase.selected_choice.min(CHOICES.len() - 1)]
        )
        .as_str(),
    )
    .with_css("margin-bottom: 10px; color: #2a6;");

    let content = Dom::create_div()
        .with_css("flex-grow: 1; overflow: auto; background: white;")
        .with_css(padding)
        .with_child(dropdown_dom)
        .with_child(selected_label)
        .with_child(button_dom)
        .with_child(checkbox_dom)
        .with_child(progress_bar)
        .with_child(increase_dom)
        .with_child(text_input)
        .with_child(number_input)
        .with_child(color_input);

    Dom::create_body()
        .with_css("display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0;")
        .with_child(ribbon(&data, showcase.active_tab))
        .with_child(content)
}

fn small(icon: &str, label: &str) -> RibbonItem {
    RibbonItem::SmallButton(RibbonButton::new(icon, label))
}

fn menu(icon: &str, label: &str) -> RibbonItem {
    RibbonItem::SmallButton(RibbonButton::new(icon, label).with_arrow(RibbonArrow::Menu))
}

fn large(icon: &str, label: &str, arrow: RibbonArrow) -> RibbonItem {
    RibbonItem::LargeButton(RibbonButton::new(icon, label).with_arrow(arrow))
}

fn row(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Row(
        items
            .into_iter()
            .fold(RibbonRow::new(), |r, it| r.with_item(it)),
    )
}

fn column(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Column(
        items
            .into_iter()
            .fold(RibbonColumn::new(), |c, it| c.with_item(it)),
    )
}

fn home_tab() -> RibbonTab {
    let clipboard = RibbonGroup::new("Clipboard")
        .with_item(large("content_paste", "Paste", RibbonArrow::Split))
        .with_item(column(vec![
            small("content_cut", "Cut"),
            small("content_copy", "Copy"),
            small("format_paint", "Format Painter"),
        ]));

    let font = RibbonGroup::new("Font").with_item(column(vec![
        row(vec![
            small("text_increase", ""),
            small("text_decrease", ""),
            menu("text_fields", ""),
            small("format_clear", ""),
        ]),
        row(vec![
            small("format_bold", ""),
            small("format_italic", ""),
            small("format_underlined", ""),
            small("strikethrough_s", ""),
            RibbonItem::Separator,
            menu("format_color_text", ""),
        ]),
    ]));

    let paragraph = RibbonGroup::new("Paragraph").with_item(column(vec![
        row(vec![
            menu("format_list_bulleted", ""),
            menu("format_list_numbered", ""),
            RibbonItem::Separator,
            small("format_indent_decrease", ""),
            small("format_indent_increase", ""),
        ]),
        row(vec![
            small("format_align_left", ""),
            small("format_align_center", ""),
            small("format_align_right", ""),
            RibbonItem::Separator,
            menu("format_line_spacing", ""),
        ]),
    ]));

    let editing = RibbonGroup::new("Editing").with_item(column(vec![
        menu("search", "Find"),
        small("find_replace", "Replace"),
        menu("highlight_alt", "Select"),
    ]));

    RibbonTab::new("HOME")
        .with_group(clipboard)
        .with_group(font)
        .with_group(paragraph)
        .with_group(editing)
}

fn insert_tab() -> RibbonTab {
    RibbonTab::new("INSERT")
        .with_group(RibbonGroup::new("Tables").with_item(large(
            "grid_on",
            "Table",
            RibbonArrow::Menu,
        )))
        .with_group(
            RibbonGroup::new("Illustrations")
                .with_item(large("image", "Pictures", RibbonArrow::None))
                .with_item(large("insert_chart", "Chart", RibbonArrow::None))
                .with_item(large("category", "Shapes", RibbonArrow::Menu)),
        )
}

fn view_tab() -> RibbonTab {
    RibbonTab::new("VIEW")
        .with_group(
            RibbonGroup::new("Views")
                .with_item(large("article", "Read Mode", RibbonArrow::None))
                .with_item(large("description", "Print Layout", RibbonArrow::None))
                .with_item(large("public", "Web Layout", RibbonArrow::None)),
        )
        .with_group(
            RibbonGroup::new("Zoom")
                .with_item(large("zoom_in", "Zoom", RibbonArrow::None))
                .with_item(large("fit_screen", "One Page", RibbonArrow::None)),
        )
}

fn ribbon(data: &RefAny, active_tab: usize) -> Dom {
    Ribbon::new(vec![home_tab(), insert_tab(), view_tab()])
        .with_app_button(RibbonAppButton::new("FILE"))
        .with_active_tab(active_tab)
        .with_on_tab_click(data.clone(), on_tab_click)
        .dom()
}

extern "C" fn on_tab_click(mut data: RefAny, _: CallbackInfo, index: usize) -> Update {
    let mut data = match data.downcast_mut::<WidgetShowcase>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    data.active_tab = index;
    Update::RefreshDom
}

extern "C" fn toggle_padding(mut data: RefAny, _: CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<WidgetShowcase>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    data.enable_padding = !data.enable_padding;
    Update::RefreshDom
}

extern "C" fn toggle_padding_checkbox(
    mut data: RefAny,
    _: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    let mut data = match data.downcast_mut::<WidgetShowcase>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    data.enable_padding = state.checked;
    Update::RefreshDom
}

extern "C" fn increase_progress(mut data: RefAny, _: CallbackInfo) -> Update {
    let mut data = match data.downcast_mut::<WidgetShowcase>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    data.progress = (data.progress + 10.0).min(100.0);
    Update::RefreshDom
}

extern "C" fn on_dropdown_change(mut data: RefAny, _: CallbackInfo, choice: usize) -> Update {
    let mut data = match data.downcast_mut::<WidgetShowcase>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    data.selected_choice = choice;
    Update::RefreshDom
}

fn main() {
    let data = RefAny::new(WidgetShowcase {
        enable_padding: true,
        progress: 20.0,
        selected_choice: 0,
        active_tab: 0,
    });
    let app = App::create(data, AppConfig::create());
    let mut options = WindowCreateOptions::create(layout);
    options.window_state.title = "Azul Widgets".into();
    options.window_state.size.dimensions.width = 900.0;
    options.window_state.size.dimensions.height = 620.0;
    app.run(options);
}
