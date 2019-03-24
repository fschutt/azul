use {
    traits::Layout,
    dom::{Dom, DomString, TabIndex},
    app_resources::ImageId,
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Button {
    pub content: ButtonContent,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ButtonContent {
    Image(ImageId),
    // Buttons should only contain short amounts of text
    Text(DomString),
}

impl Button {
    pub fn with_label<S: Into<DomString>>(text: S) -> Self {
        Self {
            content: ButtonContent::Text(text.into()),
        }
    }

    pub fn with_image(image: ImageId) -> Self {
        Self {
            content: ButtonContent::Image(image),
        }
    }

    pub fn dom<T: Layout>(self) -> Dom<T> {
        use self::ButtonContent::*;

        let mut button_root = Dom::div()
            .with_class("__azul-native-button")
            .with_tab_index(TabIndex::Auto);

        button_root.add_child(match self.content {
            Text(s) => Dom::label(s),
            Image(i) => Dom::image(i),
        });

        button_root
    }
}
