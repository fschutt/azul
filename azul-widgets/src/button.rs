use azul::{
    dom::{TabIndex, IdOrClass, IdOrClass::Class},
    style::StyledDom,
    image::ImageRef,
    css::Css,
    str::String as AzString,
    callbacks::{CallbackType, Callback, RefAny},
    vec::IdOrClassVec,
};

static CLASSES: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-button"))];

pub type OnClickFn = CallbackType;

#[derive(Debug, Clone)]
pub struct Button {
    /// Content (image or text) of this button, centered by default
    pub content: ButtonContent,
    /// Style for this button
    pub style: Css,
    /// Optional: Function to call when the button is clicked
    pub on_click: Option<(RefAny, Callback)>,
}

#[derive(Debug, Clone)]
pub enum ButtonContent {
    // Buttons displays a centered text
    Text(AzString),
    // Button displays a centered image
    Image(ImageRef),
}

impl Button {

    #[inline]
    pub fn text<S: Into<AzString>>(text: S) -> Self {
        Self {
            content: ButtonContent::Text(text.into()),
            style: Self::native_css(),
            on_click: None,
        }
    }

    #[inline]
    pub fn image(image: ImageRef) -> Self {
        Self {
            content: ButtonContent::Image(image),
            style: Self::native_css(),
            on_click: None,
        }
    }

    #[inline]
    pub fn with_style(self, css: Css) -> Self {
        Self { style: css, .. self }
    }

    /// Returns the native style for the button, differs based on operating system
    #[inline]
    pub fn native_css() -> Css {
        #[cfg(target_os = "windows")] { Self::windows_css() }
        #[cfg(target_os = "mac")] { Self::mac_css() }
        #[cfg(target_os = "linux")] { Self::linux_css() }
        #[cfg(not(any(target_os = "windows", target_os = "mac", target_os = "linux")))] { Self::web_css() }
    }

    #[inline]
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

    #[inline]
    pub fn linux_css() -> Css {
        Css::from_string("
           .__azul-native-button {
               font-size: 13px;
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

    #[inline]
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

    #[inline]
    pub fn web_css() -> Css {
        Css::empty() // TODO
    }

    #[inline]
    pub fn on_click(self, data: RefAny, on_click: OnClickFn) -> Self {
        Self {
            on_click: Some((data, Callback { cb: on_click })),
            .. self
        }
    }

    #[inline]
    pub fn dom(self) -> StyledDom {

        use self::ButtonContent::*;
        use azul::vec::DomVec;
        use azul::dom::{
            Dom, EventFilter, HoverEventFilter,
            CallbackData,
        };

        let content = match self.content {
            Text(s) => Dom::text(s),
            Image(i) => Dom::image(i),
        };

        let callbacks = match self.on_click {
            Some((data, callback)) => vec![
                CallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback,
                    data,
                }
            ],
            None => Vec::new(),
        };

        Dom::div()
        .with_ids_and_classes(IdOrClassVec::from(CLASSES))
        .with_callbacks(callbacks.into())
        .with_tab_index(Some(TabIndex::Auto).into())
        .with_children(DomVec::from(vec![content]))
        .style(self.style)
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

    let button = Button::label("Hello").dom();
    let button_html = button.get_html_string();

    if expected_html != button_html.as_str() {
        panic!("expected:\r\n{}\r\ngot:\r\n{}", expected_html, button_html);
    }
}
