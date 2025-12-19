use azul::{prelude::*, str::String as AzString, widgets::*};

#[derive(Default)]
struct WidgetShowcase {
    enable_padding: bool,
    active_tab: usize,
}

extern "C" 
fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> StyledDom {

    let showcase = match data.downcast_ref::<WidgetShowcase>() {
        Some(s) => s,
        None => return StyledDom::default(),
    };

    let enable_padding = showcase.enable_padding;
    let active_tab = showcase.active_tab;

    let text = if enable_padding {
        "Disable padding"
    } else {
        "Enable padding"
    };

    println!("layout!");

    let menu = Menu::new(vec![MenuItem::String(
        StringMenuItem::new("Menu Item 1").with_children(vec![MenuItem::String(
            StringMenuItem::new("Submenu Item 1..."),
        )]),
    )]);

    let padding = match enable_padding { 
        true => "padding: 10px",
        false => "",
    };

    Dom::create_body()
        .with_menu_bar(menu)
        .with_inline_style(padding)
        .with_child(
            TabHeader::new(vec![
                format!("Test"),
                format!("Inactive"),
                format!("Inactive 2"),
            ])
            .with_active_tab(active_tab)
            .with_on_click(data.clone(), switch_active_tab)
            .dom(),
        )
        .with_child(
            TabContent::new(match active_tab {
                0 => Frame::new(
                    "Frame",
                    Dom::create_div().with_children(vec![
                        Button::new(text)
                            .with_on_click(data.clone(), enable_disable_padding)
                            .dom()
                            .with_inline_style("margin-bottom: 5px;"),
                        CheckBox::new(enable_padding)
                            .with_on_toggle(data.clone(), enable_disable_padding_check)
                            .dom()
                            .with_inline_style("margin-bottom: 5px;"),
                        DropDown::new(Vec::<AzString>::new())
                            .dom()
                            .with_inline_style("margin-bottom: 5px;"),
                        ProgressBar::new(20.0)
                            .dom()
                            .with_inline_style("margin-bottom: 5px;"),
                        ColorInput::new(ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 255,
                        })
                        .dom()
                        .with_inline_style("margin-bottom: 5px;"),
                        TextInput::new()
                            .with_placeholder("Input text...")
                            .dom()
                            .with_inline_style("margin-bottom: 5px;"),
                        NumberInput::new(5.0)
                            .dom()
                            .with_inline_style("margin-bottom: 5px;"),
                        Dom::create_div()
                            .with_inline_style("flex-direction: row;")
                            .with_children(vec![ListView::new(vec![
                                format!("Column 1"),
                                format!("Column 2"),
                                format!("Column 3"),
                                format!("Column 4"),
                            ])
                            .with_rows(
                                (0..100)
                                    .map(|i| ListViewRow {
                                        cells: vec![Dom::create_text(format!("{}", i))].into(),
                                        height: None.into(),
                                    })
                                    .collect::<Vec<_>>(),
                            )
                            .dom()]),
                    ]),
                )
                .dom(),
                _ => Dom::create_div(),
            })
            .with_padding(enable_padding)
            .dom(),
        )
        .style(Css::empty())
}

extern "C" 
fn text_mouse_down(mut data: RefAny, info: CallbackInfo) -> Update {
    use azul::option::OptionInlineText;

    let cursor_relative_to_node = match info.get_cursor_relative_to_node().into_option() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    println!("cursor_relative_to_node: {:?}", cursor_relative_to_node);

    let inline_text = match info.get_inline_text(info.get_hit_node()) {
        OptionInlineText::Some(s) => s,
        OptionInlineText::None => return Update::DoNothing,
    };

    let hit = inline_text.hit_test(cursor_relative_to_node);

    println!("hit: {:#?}", hit);

    Update::DoNothing
}

extern "C" 
fn switch_active_tab(
    mut data: RefAny,
    _: CallbackInfo,
    h: &TabHeaderState,
) -> Update {
    match data.downcast_mut::<WidgetShowcase>() {
        Some(mut s) => {
            s.active_tab = h.active_tab;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

extern "C" 
fn enable_disable_padding_check(
    mut data: RefAny,
    _: CallbackInfo,
    c: &CheckBoxState,
) -> Update {
    match data.downcast_mut::<WidgetShowcase>() {
        Some(mut s) => {
            s.enable_padding = c.checked;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

extern "C" 
fn enable_disable_padding(mut data: RefAny, _: CallbackInfo) -> Update {
    match data.downcast_mut::<WidgetShowcase>() {
        Some(mut s) => {
            s.enable_padding = !s.enable_padding;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

fn main() {
    let data = RefAny::new(WidgetShowcase {
        enable_padding: true,
        active_tab: 0,
    });
    let app = App::new(data, AppConfig::new());
    let mut options = WindowCreateOptions::new(layout);
    options.state.flags.frame = WindowFrame::Maximized;
    app.run(options);
}
