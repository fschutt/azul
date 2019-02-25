use {
    traits::Layout,
    dom::{Dom, DomString},
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Label {
    pub text: DomString,
}

impl Label {

    pub fn new<S: Into<DomString>>(text: S) -> Self {
        Self { text: text.into() }
    }

    pub fn dom<T: Layout>(self) -> Dom<T> {
        Dom::div()
        .with_child(Dom::label(self.text))
        .with_class("__azul-native-label")
    }
}
