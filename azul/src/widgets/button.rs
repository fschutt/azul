use {
    dom::{Dom, NodeType},
    images::ImageId,
    traits::Layout,
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
    pub fn with_label<S>(text: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            content: ButtonContent::Text(text.into()),
        }
    }

    pub fn with_image(image: ImageId) -> Self {
        Self {
            content: ButtonContent::Image(image),
        }
    }

    pub fn dom<T>(self) -> Dom<T>
    where
        T: Layout,
    {
        use self::ButtonContent::*;
        let mut button_root = Dom::new(NodeType::Div).with_class("__azul-native-button");
        button_root.add_child(match self.content {
            Text(s) => Dom::new(NodeType::Label(s)),
            Image(i) => Dom::new(NodeType::Image(i)),
        });
        button_root
    }
}
