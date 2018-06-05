#![allow(non_snake_case)]

use svg::SvgLayerId;
use window::ReadOnlyWindow;
use traits::GetDom;
use traits::Layout;
use dom::{Dom, NodeType};
use images::ImageId;

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
}

impl GetDom for Button {
    fn dom<T: Layout>(self) -> Dom<T> {
        use self::ButtonContent::*;
        let mut button_root = Dom::new(NodeType::Div).with_class("__azul-native-button");
        button_root.add_child(match self.content {
            Image(i) => Dom::new(NodeType::Image(i)),
            Text(s) => Dom::new(NodeType::Label(s)),
        });
        button_root
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Svg {
    pub layers: Vec<SvgLayerId>,
}

impl Svg {
    fn dom<T: Layout>(window: &ReadOnlyWindow) -> Dom<T> {
        let mut svg_root = Dom::new(NodeType::Div).with_class("__azul-native-svg");

        // todo: implement window drawing

        svg_root
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Label {
    pub text: String,
}

impl Label {
    pub fn new<S: Into<String>>(text: S) -> Self {
        Self { text: text.into() }
    }
}

impl GetDom for Label {
    fn dom<T: Layout>(self) -> Dom<T> {
        Dom::new(NodeType::Label(self.text))
    }
}