//! Table view

use std::collections::BTreeMap;
use {
    app::AppStateNoData,
    callbacks::{IFrameCallback, HidpiAdjustedBounds, UpdateScreen, DontRedraw},
    dom::{Dom, On, NodeData, DomString, NodeType},
    callbacks::{LayoutInfo, CallbackInfo},
    callbacks::{StackCheckedPointer, DefaultCallback},
    window::FakeWindow,
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
            column_width: 100.0,
            row_height: 20.0,
            selected_cell: None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Worksheet {
    pub data: BTreeMap<usize, BTreeMap<usize, String>>,
}

impl Worksheet {
    pub fn set_cell<I: Into<String>>(&mut self, x: usize, y: usize, value: I) {
        self.data
            .entry(x)
            .or_insert_with(|| BTreeMap::new())
            .insert(y, value.into());
    }
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

    pub fn dom<T>(&self, data: &TableViewState, t: &T, window: &mut FakeWindow<T>) -> Dom<T> {
        if let Some(ptr) =  StackCheckedPointer::new(t, data) {
            let mut dom = Dom::iframe(IFrameCallback(render_table_callback), ptr);
            let callback_id = window.add_callback(ptr, DefaultCallback(Self::table_view_on_click));
            dom.add_default_callback_id(On::MouseUp, callback_id);
            dom
        } else {
            Dom::label(
                "Cannot create table from heap-allocated TableViewState, \
                 please call TableViewState::render_dom manually"
            )
        }
    }

    fn table_view_on_click<T>(ptr: &StackCheckedPointer<T>, data: &mut AppStateNoData<T>, event: &mut CallbackInfo<T>)
    -> UpdateScreen
    {
        unsafe { ptr.invoke_mut(TableViewState::on_click, data, event) }
    }
}

fn render_table_callback<T>(ptr: &StackCheckedPointer<T>, info: LayoutInfo<T>, dimensions: HidpiAdjustedBounds)
-> Dom<T>
{
    unsafe { ptr.invoke_mut_iframe(TableViewState::render, info, dimensions) }
}


impl TableViewState {
    pub fn render<T>(state: &mut TableViewState, _info: LayoutInfo<T>, dimensions: HidpiAdjustedBounds)
    -> Dom<T>
    {
        let logical_size = dimensions.get_logical_size();
        let necessary_columns = (logical_size.width as f32 / state.column_width).ceil() as usize;
        let necessary_rows = (logical_size.height as f32 / state.row_height).ceil() as usize;

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

        Dom::div()
        .with_class("__azul-native-table-container")
        .with_child(
            Dom::div()
            .with_class("__azul-native-table-row-number-wrapper")
            .with_child(
                // Empty rectangle at the top left of the table
                Dom::div()
                .with_class("__azul-native-table-top-left-rect")
            )
            .with_child(
                // Rows - "1", "2", "3"
                (0..necessary_rows.saturating_sub(1))
                .map(|row_idx|
                    NodeData::label(format!("{}", row_idx + 1))
                    .with_classes(vec![DomString::Static("__azul-native-table-row")])
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
                .with_child(Dom::label(column_name_from_number(col_idx)).with_class("__azul-native-table-column-name"))
                .with_child(
                    // Actual rows - if no content is given, they are simply empty
                    (0..necessary_rows)
                    .map(|row_idx|
                        NodeData::new(
                            if let Some(data) = state.work_sheet.data.get(&col_idx).and_then(|col| col.get(&row_idx)) {
                                NodeType::Label(DomString::Heap(data.clone()))
                            } else {
                                NodeType::Div
                            }
                        ).with_classes(vec![DomString::Static("__azul-native-table-cell")])
                    )
                    .collect::<Dom<T>>()
                    .with_class("__azul-native-table-rows")
                )
            )
            .collect::<Dom<T>>()
            .with_class("__azul-native-table-column-container")
            // current active selection (s)
            .with_child(
                Dom::div()
                    .with_class("__azul-native-table-selection")
                    .with_child(Dom::div().with_class("__azul-native-table-selection-handle"))
            )
        )
    }

    pub fn on_click<T>(
        &mut self,
        _app_state: &mut AppStateNoData<T>,
        _window_event: &mut CallbackInfo<T>)
    -> UpdateScreen
    {
        println!("table was clicked");
        DontRedraw
    }
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

    #[inline(always)]
    fn u8_to_char(input: u8) -> u8 {
        'A' as u8 + input
    }

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

#[test]
fn test_column_name_from_number() {
    assert_eq!(column_name_from_number(0), String::from("A"));
    assert_eq!(column_name_from_number(1), String::from("B"));
    assert_eq!(column_name_from_number(6), String::from("G"));
    assert_eq!(column_name_from_number(26), String::from("AA"));
    assert_eq!(column_name_from_number(27), String::from("AB"));
    assert_eq!(column_name_from_number(225), String::from("HR"));
}