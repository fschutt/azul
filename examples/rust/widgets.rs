#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::prelude::*;
use azul::widgets::*;

#[derive(Default)]
struct WidgetShowcase {
}

extern "C" fn layout(data: &mut RefAny, _: LayoutCallbackInfo) -> StyledDom {
    let mut dom = Dom::body()
    .with_menu_bar(Menu::new(vec![
        MenuItem::String(StringMenuItem::new("Menu Item 1".into()).with_children(vec![
            MenuItem::String(StringMenuItem::new("Submenu Item 1...".into()))
        ].into()))
    ].into()));

    dom.add_child(
        TabContainer::new(vec![
            Tab {
                title: "Test".into(),
                content: Dom::div()
                .with_children(vec![
                    Button::new("Hello".into()).dom(),
                    CheckBox::new(false).dom(),
                    ProgressBar::new(50.0).dom(),
                    ColorInput::new(ColorU { r: 0, g: 0, b: 0, a: 255 }).dom(),
                    TextInput::new("Input text...".into()).dom(),
                    NumberInput::new(5.0).dom(),
                ].into())
            },
            Tab {
                title: "Inactive".into(),
                content: Dom::div()
            },
            Tab {
                title: "Inactive 2".into(),
                content: Dom::div()
            }
        ].into())
        .with_padding(false)
        .dom()
    );

    dom.style(Css::empty())
}

fn main() {
    let data = RefAny::new(WidgetShowcase::default());
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut options = WindowCreateOptions::new(layout);
    options.state.flags.frame = WindowFrame::Maximized;
    app.run(options);
}
