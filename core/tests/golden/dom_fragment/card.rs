use azul::prelude::*;

pub fn render_card() -> Dom {
    Dom::create_div()
        .with_css("padding: 8px")
        .with_class("card")
        .with_children(vec![
            Dom::create_h1_with_text("Title"),
            Dom::create_p()
                .with_children(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper("Hello "),
                    Dom::create_b_with_text("world"),
                ]),
            Dom::create_a_no_a11y("https://azul.rs", OptionString::some("Docs")),
            Dom::create_button_no_a11y("Go"),
        ])
}
