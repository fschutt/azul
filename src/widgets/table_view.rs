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
    // Column header with column  - "A", "B", "C"
    .with_child(
        (0..data.columns.len())
            .map(|column_idx| NodeType::Label(column_name_from_number(column_idx)))
            .collect::<Dom<T>>()
            .with_class("__azul-native-table-header")
    )
    .with_child(data.columns.iter().map(|column| {
        column.cells.iter().map(|cell| {
            NodeData { node_type: NodeType::Label(cell.clone()), .. Default::default() }
        }).collect::<Dom<T>>()
        .with_class("__azul-native-table-column")
    }).collect())
}

/// Maps an index number to a value, necessary for creating the column name:
///
/// ```no_run,ignore
/// 0   -> A
/// 25  -> Z
/// 26  -> AA
/// 27  -> AB
/// ```
///
/// ... and so on. This implementation is very fast, takes ~50 to 100
/// nanoseconds for 1 iteration due to almost pure-stack allocated data.
/// For an explanation of the algorithm with comments, see:
/// https://github.com/fschutt/street_index/blob/78b935a1303070947c0854b6d01f540ec298c9d5/src/gridconfig.rs#L155-L209
pub fn column_name_from_number(num: usize) -> String {
    const ALPHABET_LEN: usize = 26;
    // usize::MAX is "GKGWBYLWRXTLPP" with a length of 15 characters
    const MAX_LEN: usize = 15;

    let mut result = [0;MAX_LEN + 1];
    let mut multiple_of_alphabet = num / ALPHABET_LEN;
    let mut character_count = 0;

    while multiple_of_alphabet != 0 && character_count < MAX_LEN {
        let remainder = (multiple_of_alphabet - 1) % ALPHABET_LEN;
        result[(MAX_LEN - 1) - character_count] = u8_to_char(remainder as u8);
        character_count += 1;
        multiple_of_alphabet = (multiple_of_alphabet - 1) / ALPHABET_LEN;
    }

    result[MAX_LEN] = u8_to_char((num % ALPHABET_LEN) as u8);
    let zeroed_characters = MAX_LEN.saturating_sub(character_count);
    let slice = &result[zeroed_characters..];
    unsafe { ::std::str::from_utf8_unchecked(slice) }.to_string()
}

#[inline(always)]
fn u8_to_char(input: u8) -> u8 {
    'A' as u8 + input
}

#[test]
fn test_column_name_from_number() {
    assert_eq!(column_name_from_number(0), String::from("A"));
    assert_eq!(column_name_from_number(1), String::from("B"));
    assert_eq!(column_name_from_number(6), String::from("G"));
    assert_eq!(column_name_from_number(26), String::from("AA"));
    assert_eq!(column_name_from_number(27), String::from("AB"));
    assert_eq!(column_name_from_number(225), String::from("HR"));
}