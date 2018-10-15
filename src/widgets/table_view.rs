//! Table view

use std::collections::BTreeMap;
use {
    dom::{Dom, NodeData, NodeType, IFrameCallback},
    traits::Layout,
    window::WindowInfo,
    default_callbacks::StackCheckedPointer,
    window::HidpiAdjustedBounds,
};

#[derive(Debug, Default, Copy, Clone)]
pub struct TableView {

}

#[derive(Debug, Clone)]
pub struct TableViewState {
    pub work_sheet: Worksheet,
    pub column_width: f32,
    pub row_height: f32,
    pub selected_cell: Option<(usize, usize)>,
}

impl Default for TableViewState {
    fn default() -> Self {
        Self {
            work_sheet: Worksheet::default(),
            column_width: 50.0,
            row_height: 10.0,
            selected_cell: None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Worksheet {
    pub data: BTreeMap<usize, BTreeMap<usize, String>>,
}

#[derive(Debug, Default, Clone)]
pub struct TableColumn {
    cells: Vec<String>,
}

impl TableView {
    pub fn new() -> Self {
        Self {

        }
    }

    pub fn dom<T: Layout>(&self, data: &TableViewState, t: &T) -> Dom<T> {
        Dom::new(NodeType::IFrame((IFrameCallback(render_table_callback), StackCheckedPointer::new(t, data).unwrap())))
    }
}

fn render_table_callback<T: Layout>(ptr: &StackCheckedPointer<T>, info: WindowInfo<T>, dimensions: HidpiAdjustedBounds)
-> Dom<T>
{
    unsafe { ptr.invoke_mut_iframe(render_table, info, dimensions) }
}

fn render_table<T: Layout>(state: &mut TableViewState, _info: WindowInfo<T>, dimensions: HidpiAdjustedBounds)
-> Dom<T>
{
    let necessary_columns = (dimensions.logical_size.width as f32 / state.column_width).ceil() as usize;
    let necessary_rows = (dimensions.logical_size.height as f32 / state.row_height).ceil() as usize;

    // div.__azul-native-table-container
    //     |-> div.__azul-native-table-column (Column 0)
    //         |-> div.__azul-native-table-top-left-rect .__azul-native-table-column-name
    //         '-> div.__azul-native-table-row-numbers .__azul-native-table-row
    //
    //     |-> div.__azul-native-table-column-container
    //         |-> div.__azul-native-table-column (Column 1 ...)
    //             |-> div.__azul-native-table-column-name
    //             '-> div.__azul-native-table-row
    //                 '-> div.__azul-native-table-cell

    Dom::new(NodeType::Div)
    .with_class("__azul-native-table-container")
    .with_child(
        Dom::new(NodeType::Div)
        .with_class("__azul-native-table-row-number-wrapper")
        .with_child(
            // Empty rectangle at the top left of the table
            Dom::new(NodeType::Div)
            .with_class("__azul-native-table-top-left-rect")
        )
        .with_child(
            // Rows - "1", "2", "3"
            (0..necessary_rows.saturating_sub(1))
            .map(|row_idx|
                NodeData {
                    node_type: NodeType::Label(format!("{}", row_idx + 1)),
                    classes: vec![String::from("__azul-native-table-row")],
                    .. Default::default()
                }
            )
            .collect::<Dom<T>>()
            .with_class("__azul-native-table-row-numbers")
        )
    )
    .with_child(
        (0..necessary_columns)
        .map(|col_idx|
            // Column name
            Dom::new(NodeType::Div)
            .with_class("__azul-native-table-column")
            .with_child(Dom::new(NodeType::Label(column_name_from_number(col_idx))).with_class("__azul-native-table-column-name"))
            .with_child(
                // Actual rows - if no content is given, they are simply empty
                (0..necessary_rows)
                .map(|row_idx|
                    NodeData {
                        node_type: if let Some(data) = state.work_sheet.data.get(&col_idx).and_then(|col| col.get(&row_idx)) {
                            NodeType::Label(data.clone())
                        } else {
                            NodeType::Div
                        },
                        classes: vec![String::from("__azul-native-table-cell")],
                        .. Default::default()
                    }
                )
                .collect::<Dom<T>>()
                .with_class("__azul-native-table-rows")
            )
        )
        .collect::<Dom<T>>()
        .with_class("__azul-native-table-column-container")
    )
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