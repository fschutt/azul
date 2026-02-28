#[cfg(test)]
use azul_css::css::Css;

static EXPECTED_1: &str =
    "
<div data-az-node-id=\"0\"  class=\"__azul-native-button-container\"  tabindex=\"0\"  \
     style=\"background: linear-gradient(from top to bottom, 0%#efefefff, 100%#e5e5e5ff);cursor: \
     pointer;border-top-color: #acacacff;border-left-color: #acacacff;border-right-color: \
     #acacacff;border-bottom-color: #acacacff;border-top-style: solid;border-left-style: \
     solid;border-right-style: solid;border-bottom-style: solid;display: flex;padding-top: \
     5px;padding-bottom: 5px;padding-left: 5px;padding-right: 5px;border-top-width: \
     1px;border-left-width: 1px;border-right-width: 1px;border-bottom-width: 1px;flex-direction: \
     column;flex-grow: 1;justify-content: center;\">
<p data-az-node-id=\"1\"  class=\"__azul-native-button-content\"  style=\"font-size: \
     13px;font-family: sans-serif;text-align: center;\">Hello</p>
</div>";

#[test]
fn test_button_ui_1() {
    use azul_dll::widgets::button::Button;
    use azul_core::styled_dom::StyledDom;

    let mut dom = Button::new("Hello".into())
        .dom();
    let button = StyledDom::create(&mut dom, Css::empty());
    let button_html = button.get_html_string("", "", true);

    assert_lines(EXPECTED_1.trim(), button_html.as_str().trim());
}

// assert that two strings are the same, independent of line ending format
fn assert_lines(a: &str, b: &str) {
    for (line_a, line_b) in a.lines().zip(b.lines()) {
        assert_eq!(line_a, line_b);
    }
}
