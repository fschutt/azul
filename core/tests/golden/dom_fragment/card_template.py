import azul

def render_card(title="Hello", text="Some text", href="https://azul.rs", author="me"):
    return (
        azul.Dom.create_div()
            .with_class("card")
            .with_child(azul.Dom.create_h2_with_text(title))
            .with_child(azul.Dom.create_p_with_text(text))
            .with_child(azul.Dom.create_a(href, "Read more", azul.SmallAriaInfo.label("Read more")))
            .with_child(azul.Dom.create_span_with_text(f"by {author}"))
    )
