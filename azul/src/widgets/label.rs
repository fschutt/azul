use {
    dom::{Dom, NodeType},
    traits::Layout,
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Label {
    pub text: String,
}

impl Label {
    pub fn new<S>(text: S) -> Self
    where
        S: Into<String>,
    {
        Self { text: text.into() }
    }

    pub fn dom<T>(self) -> Dom<T>
    where
        T: Layout,
    {
        Dom::new(NodeType::Div)
            .with_child(Dom::new(NodeType::Label(self.text)))
            .with_class("__azul-native-label")
    }
}
