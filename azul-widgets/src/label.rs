use azul::{
    dom::Dom,
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Label {
    string: String,
}

impl Label {

    #[inline]
    pub fn new<S: Into<String>>(string: S) -> Self {
        Self { string: string.into() }
    }

    #[inline]
    pub fn dom(self) -> Dom {
        Dom::label(self.string.into()).with_class("__azul-native-label")
    }
}

impl Into<Dom> for Label {
    fn into(self) -> Dom {
        self.dom()
    }
}