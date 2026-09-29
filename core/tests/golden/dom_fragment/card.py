import azul

def render_card():
    return (
        azul.Dom.create_div()
            .with_css("padding: 8px")
            .with_class("card")
            .with_child(azul.Dom.create_h1_with_text("Title"))
            .with_child(azul.Dom.create_p()
                .with_child(azul.Dom.create_text_do_not_use_without_block_level_wrapper("Hello "))
                .with_child(azul.Dom.create_b_with_text("world")))
            .with_child(azul.Dom.create_a("https://azul.rs", "Docs", azul.SmallAriaInfo.label("Docs")))
            .with_child(azul.Dom.create_button_no_a11y("Go"))
    )
