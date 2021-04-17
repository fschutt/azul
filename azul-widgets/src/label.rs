use azul::{
    style::StyledDom,
    css::Css,
    str::String as AzString,
};

#[derive(Debug, Clone)]
pub struct Label {
    pub string: AzString,
    pub style: Css,
}

impl Label {

    #[inline]
    pub fn new<S: Into<AzString>>(string: S) -> Self {
        Self {
            string: string.into(),
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
            .__azul-native-label {
                display: flex;
                box-sizing: border-box;
                font-size: 13px;
                flex-grow: 1;
                font-family: sans-serif;
                flex-direction: row;
                text-align: center;
                justify-content: center;
                align-items: center;
            }".into()
        )
    }

    pub fn linux_css() -> Css {
        Css::from_string("
           .__azul-native-label {
               font-size: 13px;
               font-family: sans-serif;
               color: #4c4c4c;
               display: flex;
               flex-grow: 1;
               flex-direction: row;
               text-align: center;
               justify-content: center;
               align-items: center;
           }".into()
        )
    }

    pub fn mac_css() -> Css {
        Css::from_string("
            .__azul-native-label {
                font-size: 12px;
                font-family: \"Helvetica\";
                color: #4c4c4c;
                flex-direction: row;
                text-align: center;
                justify-content: center;
                align-items: center;
            }".into()
        )
    }

    pub fn web_css() -> Css {
        Css::empty() // TODO
    }

    #[inline]
    pub fn dom(self) -> StyledDom {

        use azul::vec::{IdOrClassVec};
        use azul::dom::{Dom, IdOrClass, IdOrClass::Class};

        static CLASSES: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-label"))];

        let dom = Dom::text(self.string)
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLASSES));

        StyledDom::new(dom, self.style)
    }
}

impl From<Label> for StyledDom {
    fn from(l: Label) -> StyledDom {
        l.dom()
    }
}