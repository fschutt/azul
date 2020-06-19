use azul::{
    dom::{Dom, TabIndex},
    resources::ImageId,
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Button {
    pub content: ButtonContent,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ButtonContent {
    Image(ImageId),
    // Buttons should only contain short amounts of text
    Text(String),
}

impl Button {
    pub fn with_label<S: Into<String>>(text: S) -> Self {
        Self {
            content: ButtonContent::Text(text.into()),
        }
    }

    pub fn with_image(image: ImageId) -> Self {
        Self {
            content: ButtonContent::Image(image),
        }
    }

    pub fn dom(self) -> Dom {
        use self::ButtonContent::*;
        Dom::div()
        .with_class("__azul-native-button".into())
        .with_tab_index(Some(TabIndex::Auto).into())
        .with_child(match self.content {
            Text(s) => Dom::label(s.into()),
            Image(i) => Dom::image(i),
        })
    }
}

impl Into<Dom> for Button {
    fn into(self) -> Dom {
        self.dom()
    }
}

#[test]
fn test_button_ui_1() {
    let expected_html = "<div class=\"__azul-native-button\" tabindex=\"0\">\r\n    <p>\r\n        Hello\r\n    </p>\r\n</div>";

    let button = Button::with_label("Hello").dom();
    let button_html = button.get_html_string();

    if expected_html != button_html {
        panic!("expected:\r\n{}\r\ngot:\r\n{}", expected_html, button_html);
    }
}
