use {
    traits::Layout,
    dom::{Dom, DomString},
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Label {
    pub text: DomString,
}

impl Label {

    #[inline]
    pub fn new<S: Into<DomString>>(text: S) -> Self {
        Self { text: text.into() }
    }

    #[inline]
    pub fn dom<T: Layout>(self) -> Dom<T> {
        Dom::label(self.text).with_class("__azul-native-label")
    }
}
