#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use azul::prelude::*;
use azul::widgets::*;
use azul::str::String as AzString;

#[derive(Default)]
struct WidgetShowcase {
    enable_padding: bool,
    active_tab: usize,
}

extern "C" fn layout(data: &mut RefAny, _: &mut LayoutCallbackInfo) -> StyledDom {

    let (enable_padding, active_tab) = match data.downcast_ref::<WidgetShowcase>() {
        Some(s) => (s.enable_padding, s.active_tab),
        None => return StyledDom::default(),
    };

    let text = if enable_padding {
        "Disable padding"
    } else {
        "Enable padding"
    };

    println!("layout!");
    
    Dom::body()
    .with_menu_bar(Menu::new(vec![
        MenuItem::String(StringMenuItem::new("Menu Item 1").with_children(vec![
            MenuItem::String(StringMenuItem::new("Submenu Item 1..."))
        ]))
    ]))
    .with_inline_style(if enable_padding {
        "padding: 10px"
    } else {
        ""
    })
    .with_child(
        TabHeader::new(vec![format!("Test"), format!("Inactive"), format!("Inactive 2")])
        .with_active_tab(active_tab)
        .with_on_click(data.clone(), switch_active_tab)
        .dom()
    ).with_child(
        TabContent::new(match active_tab {
            0 => Frame::new("Frame",
                Dom::div()
                .with_children(vec![
                    Button::new(text)
                    .with_on_click(data.clone(), enable_disable_padding)
                    .dom()
                    .with_inline_style("margin-bottom: 5px;"),

                    CheckBox::new(enable_padding)
                    .with_on_toggle(data.clone(), enable_disable_padding_check)
                    .dom()
                    .with_inline_style("margin-bottom: 5px;"),

                    DropDown::new(Vec::<AzString>::new()).dom()
                    .with_inline_style("margin-bottom: 5px;"),

                    ProgressBar::new(20.0).dom()
                    .with_inline_style("margin-bottom: 5px;"),

                    ColorInput::new(ColorU { r: 0, g: 0, b: 0, a: 255 }).dom()
                    .with_inline_style("margin-bottom: 5px;"),

                    TextInput::new()
                    .with_placeholder("Input text...")
                    .dom()
                    .with_inline_style("margin-bottom: 5px;"),

                    NumberInput::new(5.0).dom()
                    .with_inline_style("margin-bottom: 5px;"),

                    Dom::div()
                    .with_inline_style("flex-direction: row;")
                    .with_children(vec![
                        ListView::new(vec![
                            format!("Column 1"),
                            format!("Column 2"),
                            format!("Column 3"),
                            format!("Column 4"),
                        ])
                        .with_rows((0..100).map(|i| {
                            ListViewRow {
                                cells: vec![Dom::text(format!("{}", i))].into(),
                                height: None.into(),
                            }
                        }).collect::<Vec<_>>())
                        .dom(),
                    ]),

                    /*
                    Dom::div()
                    .with_inline_style("flex-direction: row;padding:10px;")
                    .with_children(vec![
                        Dom::div()
                        .with_children(vec![
                            Dom::text("This is a text 1 placerat urna, tristique sodales nisi. Fusce \
                              at commodo ipsum. Etiam consequat, metus rutrum porttitor tempor,  \
                              metus ante luctus est, sed iaculis turpis risus vel neque. Phasellus \
                              cursus, dolor semper ultricies ele")
                            .with_inline_style("
                                font-family:Courier New Bold;
                                font-size:25px;
                                display:inline;
                            "),
                            Dom::text("\
                              Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
                              Nulla gravida placerat urna, tristique sodales nisi. Fusce \
                              at commodo ipsum. Etiam consequat, metus rutrum porttitor tempor,  \
                              metus ante luctus est, sed iaculis turpis risus vel neque. Phasellus \
                              cursus, dolor semper ultricies eleifend, erat magna faucibus nisl, ut \
                              interdum velit magna quis neque. Integer mollis eros nec leo faucibus, \
                              nec tincidunt dolor posuere.")
                            .with_inline_style("
                                font-family:serif;
                                font-size:26px;
                                display:inline;
                                background: red;
                                color:white;
                            ")
                            .with_callback(
                                On::LeftMouseDown.into_event_filter(),
                                data.clone(),
                                text_mouse_down
                            ),
                            Dom::text("3")
                            .with_inline_style("font-size:10px"),
                            Dom::text("2")
                            .with_inline_style("font-size:5px"),
                        ])
                    ])*/

                ])
            ).dom(),
            _ => Dom::div(),
        })
        .with_padding(enable_padding)
        .dom()
    ).style(Css::empty())
}

extern "C" fn text_mouse_down(data: &mut RefAny, info: &mut CallbackInfo) -> Update {

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

extern "C" fn switch_active_tab(data: &mut RefAny, _: &mut CallbackInfo, h: &TabHeaderState) -> Update {
    match data.downcast_mut::<WidgetShowcase>() {
        Some(mut s) => { s.active_tab = h.active_tab; Update::RefreshDom },
        None => Update::DoNothing,
    }
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
    let data = RefAny::new(WidgetShowcase { enable_padding: true, active_tab: 0 });
    let app = App::new(data, AppConfig::new(LayoutSolver::Default));
    let mut options = WindowCreateOptions::new(layout);
    options.state.flags.frame = WindowFrame::Maximized;
    app.run(options);
}
