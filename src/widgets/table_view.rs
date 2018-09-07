//! Table view

use {
    dom::{Dom, NodeType, IFrameCallback},
    traits::Layout,
    window::WindowInfo,
};

#[derive(Debug, Default, Copy, Clone)]
pub struct TableView {

}

#[derive(Debug, Default, Copy, Clone)]
pub struct TableViewOutcome {

}

impl TableView {
    pub fn new() -> Self {
        Self { }
    }

    pub fn dom<T: Layout>(&self) -> Dom<T> {
        Dom::new(NodeType::IFrame(IFrameCallback(render_table)))
    }
}

fn render_table<T: Layout>(data: &T, info: WindowInfo<T>, width: usize, height: usize) 
-> Dom<T> 
{
    println!("rendering DOM @ {} x {}", width, height);
    Dom::new(NodeType::Div).with_class("__azul-native-table-container")
}