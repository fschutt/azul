#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::prelude::*;
use azul::widgets::*;

#[derive(Default)]
struct WidgetShowcase {
}

extern "C" fn layout(data: &mut RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let mut dom = Dom::body();
    dom.add_child(Button::new("Hello".into()).dom());
    dom.add_child(CheckBox::new(false).dom());
    dom.add_child(ProgressBar::new(50.0).dom());
    dom.add_child(TextInput::new("Input text...".into()).dom());
    dom.add_child(NumberInput::new(5.0).dom());
    return dom.style(Css::empty());
}

fn main() {
    let data = RefAny::new(WidgetShowcase::default());
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut options = WindowCreateOptions::new(layout);
    options.state.flags.frame = WindowFrame::Maximized;
    app.run(options);
}
