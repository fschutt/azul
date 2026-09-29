use azul::prelude::*;

pub fn render_card(title: &str, text: &str, href: &str, author: &str) -> Dom {
    Dom::create_div()
        .with_class("card")
        .with_children(vec![
            Dom::create_h2_with_text(title),
            Dom::create_p_with_text(text),
            Dom::create_a_no_a11y(href, OptionString::some("Read more")),
            Dom::create_span_with_text(format!("by {author}")),
        ])
}
