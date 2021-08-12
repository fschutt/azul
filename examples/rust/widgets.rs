#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::prelude::*;
use azul::widgets::*;
use azul::str::String as AzString;

#[derive(Default)]
struct WidgetShowcase {
    enable_padding: bool,
}

extern "C" fn layout(data: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {

    let enable_padding = match data.downcast_ref::<WidgetShowcase>() {
        Some(s) => s.enable_padding,
        None => return StyledDom::default(),
    };

    let mut dom = Dom::body()
    .with_menu_bar(Menu::new(vec![
        MenuItem::String(StringMenuItem::new("Menu Item 1".into()).with_children(vec![
            MenuItem::String(StringMenuItem::new("Submenu Item 1...".into()))
        ].into()))
    ].into()));

    let text = if enable_padding {
        "Disable padding"
    } else {
        "Enable padding"
    };

    dom.add_child(
        TabContainer::new(vec![
            Tab {
                title: "Test".into(),
                content: Frame::new("Frame".into(),
                    Dom::div()
                    .with_children(vec![
                        Button::new(text.into())
                        .with_on_click(data.clone(), enable_disable_padding)
                        .dom(),
                        CheckBox::new(enable_padding)
                            .with_on_toggle(data.clone(), enable_disable_padding_check)
                            .dom(),
                        ProgressBar::new(20.0).dom(),
                        ColorInput::new(ColorU { r: 0, g: 0, b: 0, a: 255 }).dom(),
                        TextInput::new("Input text...".into()).dom(),
                        NumberInput::new(5.0).dom(),
                        ListView::new(Vec::<AzString>::new().into()).dom(),
                    ].into())
                ).dom()
            },
            Tab {
                title: "Inactive".into(),
                content: Dom::div(),
            },
            Tab {
                title: "Inactive 2".into(),
                content: Dom::div()
            }
        ].into())
        .with_padding(enable_padding)
        .dom()
    );

    dom.style(Css::empty())
}

extern "C" fn enable_disable_padding_check(data: &mut RefAny, _: &mut CallbackInfo, c: &CheckBoxState) -> Update {
    match data.downcast_mut::<WidgetShowcase>() {
        Some(mut s) => { s.enable_padding = c.checked; Update::RefreshDom },
        None => Update::DoNothing,
    }
}

extern "C" fn enable_disable_padding(data: &mut RefAny, _: &mut CallbackInfo) -> Update {
    match data.downcast_mut::<WidgetShowcase>() {
        Some(mut s) => { s.enable_padding = !s.enable_padding; Update::RefreshDom },
        None => Update::DoNothing,
    }
}

fn main() {
    let data = RefAny::new(WidgetShowcase { enable_padding: true });
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut options = WindowCreateOptions::new(layout);
    options.state.flags.frame = WindowFrame::Maximized;
    app.run(options);
}
