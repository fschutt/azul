use azul::{
    dom::TabIndex,
    style::StyledDom,
    resources::ImageId,
    css::Css,
    str::String as AzString,
};

#[derive(Debug, Clone)]
pub struct Button {
    pub content: ButtonContent,
    pub style: Css,
}

#[derive(Debug, Clone)]
pub enum ButtonContent {
    Image(ImageId),
    // Buttons should only contain short amounts of text
    Text(AzString),
}

impl Button {

    pub fn label<S: Into<AzString>>(text: S) -> Self {
        Self {
            content: ButtonContent::Text(text.into()),
            style: Self::native_css(),
        }
    }

    pub fn image(image: ImageId) -> Self {
        Self {
            content: ButtonContent::Image(image),
            style: Self::native_css(),
        }
    }

    pub fn with_style(self, css: Css) -> Self {
        Self { style: css, .. self }
    }

    /// Returns the native style for the button, differs based on operating system
    pub fn native_css() -> Css {
        #[cfg(target_os = "windows")] { Self::windows_css() }
        #[cfg(target_os = "mac")] { Self::mac_css() }
        #[cfg(target_os = "linux")] { Self::linux_css() }
        #[cfg(not(any(target_os = "windows", target_os = "mac", target_os = "linux")))] { Self::web_css() }
    }

    pub fn windows_css() -> Css {
        Css::from_string("
            .__azul-native-button {
                display: flex;
                box-sizing: border-box;
                font-size: 13px;
                border: 1px solid rgb(172, 172, 172);
                background: linear-gradient(to bottom, rgb(239, 239, 239), rgb(229, 229, 229));
                text-align: center;
                flex-direction: column;
                justify-content: center;
                align-items: center;
                cursor: pointer;
                flex-grow: 1;
                font-family: sans-serif;
                padding: 5px;
            }

            .__azul-native-button:hover {
                background: linear-gradient(to bottom, rgb(234, 243, 252), rgb(126, 180, 234));
                border: 1px solid rgb(126, 180, 234);
            }

            .__azul-native-button:active {
                background: linear-gradient(to bottom, rgb(217, 235, 252), rgb(86, 157, 229));
                border: 1px solid rgb(86, 157, 229);
            }

            .__azul-native-button:focus {
                border: 1px solid rgb(51, 153, 255);
            }".into()
        )
    }

    pub fn linux_css() -> Css {
        Css::from_string("
           .__azul-native-button {
               font-size: 16px;
               font-family: sans-serif;
               color: #4c4c4c;
               display: flex;
               flex-grow: 1;
               border: 1px solid #b7b7b7;
               border-radius: 4px;
               box-shadow: 0px 0px 3px #c5c5c5ad;
               background: linear-gradient(#fcfcfc, #efefef);
               text-align: center;
               flex-direction: column;
               justify-content: center;
               flex-grow: 1;
           }

           .__azul-native-button:hover {
               background: linear-gradient(red, black);
           }

           .__azul-native-button:active {
               background: linear-gradient(blue, green);
           }".into()
        )
    }

    pub fn mac_css() -> Css {
        Css::from_string("
            .__azul-native-button {
                font-size: 12px;
                font-family: \"Helvetica\";
                color: #4c4c4c;
                background-color: #e7e7e7;
                border: 1px solid #b7b7b7;
                border-radius: 4px;
                box-shadow: 0px 0px 3px #c5c5c5ad;
                background: linear-gradient(#fcfcfc, #efefef);
                text-align: center;
                flex-direction: column;
                justify-content: center;
            }".into()
        )
    }

    pub fn web_css() -> Css {
        Css::empty() // TODO
    }

    pub fn dom(self) -> StyledDom {
        use self::ButtonContent::*;
        use azul::dom::Dom;

        let content = match self.content {
            Text(s) => Dom::label(s),
            Image(i) => Dom::image(i),
        };

        let dom = Dom::div()
        .with_class("__azul-native-button".into())
        .with_tab_index(Some(TabIndex::Auto).into())
            .with_child(content);

        StyledDom::new(dom, self.style)
    }
}

impl From<Button> for StyledDom {
    fn from(b: Button) -> StyledDom {
        b.dom()
    }
}

#[test]
fn test_button_ui_1() {
    let expected_html = "<div class=\"__azul-native-button\" tabindex=\"0\">\r\n    <p>\r\n        Hello\r\n    </p>\r\n</div>";

    let button = Button::with_label("Hello").dom();
    let button_html = button.get_html_string();

    if expected_html != button_html.as_str() {
        panic!("expected:\r\n{}\r\ngot:\r\n{}", expected_html, button_html);
    }
}
