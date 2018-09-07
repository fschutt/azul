//! Table view

use {
    dom::{Dom, NodeData, NodeType, IFrameCallback},
    traits::Layout,
    window::WindowInfo,
    default_callbacks::StackCheckedPointer,
};

#[derive(Debug, Default, Copy, Clone)]
pub struct TableView {

}

#[derive(Debug, Default, Clone)]
pub struct TableViewOutcome {
    columns: Vec<TableColumn>,
}

#[derive(Debug, Default, Clone)]
pub struct TableColumn {
    cells: Vec<String>,
}

impl TableView {
    pub fn new() -> Self {
        Self { }
    }

    pub fn dom<T: Layout>(&self, data: &TableViewOutcome, t: &T) -> Dom<T> {
        Dom::new(NodeType::IFrame((IFrameCallback(render_table_callback), StackCheckedPointer::new(t, data).unwrap())))
    }
}

fn render_table_callback<T: Layout>(ptr: &StackCheckedPointer<T>, info: WindowInfo<T>, width: usize, height: usize)
-> Dom<T>
{
    unsafe { ptr.invoke_mut_iframe(render_table, info, width, height) }
}

fn render_table<T: Layout>(data: &mut TableViewOutcome, info: WindowInfo<T>, width: usize, height: usize)
-> Dom<T>
{
    Dom::new(NodeType::Div).with_class("__azul-native-table-container")
    .with_child(data.columns.iter().map(|column| {
        column.cells.iter().map(|cell| {
            NodeData { node_type: NodeType::Label(cell.clone()), .. Default::default() }
        }).collect::<Dom<T>>()
        .with_class("__azul-native-table-column")
    }).collect())
}