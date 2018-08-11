use {
    traits::Layout,
    dom::{Dom, NodeType},
};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Label {
    pub text: String,
}

impl Label {
    pub fn new<S>(text: S)
    -> Self where S: Into<String>
    {
        Self { text: text.into() }
    }

    pub fn dom<T>(self)
    -> Dom<T> where T: Layout
    {
        Dom::new(NodeType::Div)
            .with_child(Dom::new(NodeType::Label(self.text)))
        .with_class("__azul-native-label")
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_widgets_label_file() {

}