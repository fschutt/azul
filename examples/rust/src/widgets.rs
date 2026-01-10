use azul::css::ColorU;
use azul::prelude::*;
use azul::widgets::*;

#[derive(Default, Clone)]
struct WidgetShowcase {
    enable_padding: bool,
    progress: f32,
}

extern "C" fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let showcase = match data.downcast_ref::<WidgetShowcase>() {
        Some(s) => (*s).clone(),
        None => return StyledDom::default(),
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

    // Button
    let mut button = Button::create(toggle_text);
    button.set_on_click(data.clone(), toggle_padding);
    let button_dom = button.dom().with_inline_style(margin);

    // Checkbox
    let mut checkbox = CheckBox::create(enable_padding);
    checkbox.set_on_toggle(data.clone(), toggle_padding_checkbox);
    let checkbox_dom = checkbox.dom().with_inline_style("margin-bottom: 10px;");

    // Progress Bar
    let progress_bar = ProgressBar::create(progress)
        .dom()
        .with_inline_style("margin-bottom: 10px; width: 200px;");

    // Text Input
    let text_input = TextInput::create()
        .with_placeholder("Type something...")
        .dom()
        .with_inline_style(margin);

    // Number Input
    let number_input = NumberInput::create(42.0).dom().with_inline_style(margin);

    // Color Input
    let color_input = ColorInput::create(ColorU::from_str("#FF5733"))
        .dom()
        .with_inline_style(margin);

    // Increase progress button (with callback)
    let mut increase_button = Button::create("Increase Progress");
    increase_button.set_on_click(data.clone(), increase_progress);
    let increase_dom = increase_button.dom().with_inline_style(margin);

    // Heading
    let heading = Dom::create_p()
        .with_child(Dom::create_text("Widget Showcase"))
        .with_inline_style("font-size: 24px; font-weight: bold; margin-bottom: 20px;");

    // Final DOM composition
    Dom::create_body()
        .with_inline_style(padding)
        .with_child(heading)
        .with_child(button_dom)
        .with_child(checkbox_dom)
        .with_child(progress_bar)
        .with_child(increase_dom)
        .with_child(text_input)
        .with_child(number_input)
        .with_child(color_input)
        .style(Css::empty())
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

fn main() {
    let data = RefAny::new(WidgetShowcase {
        enable_padding: true,
        progress: 20.0,
    });
    let app = App::create(data, AppConfig::create());
    let options = WindowCreateOptions::create(layout);
    app.run(options);
}
