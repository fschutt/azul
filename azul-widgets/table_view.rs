//! Table view

use std::{ops::Range, collections::BTreeMap};
use azul_core::{
    dom::{Dom, On, NodeData, DomString, NodeType},
    callbacks::{
        Ref, DefaultCallback, DefaultCallbackInfo, CallbackReturn,
        IFrameCallbackInfo, IFrameCallbackReturn, DontRedraw,
    },
};

#[derive(Debug, Clone)]
pub struct TableView<T> {
    pub state: Ref<TableViewState>,
    pub on_mouse_up: DefaultCallback<T>,
}

impl<T> Default for TableView<T> {
    fn default() -> Self {
        Self {
            state: Ref::default(),
            on_mouse_up: DefaultCallback(Self::default_on_mouse_up),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TableViewState {
    pub work_sheet: BTreeMap<usize, BTreeMap<usize, String>>,
    pub column_width: f32,
    pub row_height: f32,
    pub selected_cell: Option<(usize, usize)>,
}

impl Default for TableViewState {
    fn default() -> Self {
        Self {
            work_sheet: BTreeMap::default(),
            column_width: 100.0,
            row_height: 20.0,
            selected_cell: None,
        }
    }
}

impl TableViewState {

    /// Renders a cutout of the table from, horizontally from (col_start..col_end)
    /// and vertically from (row_start..row_end)
    pub fn render<T>(&self, rows: Range<usize>, columns: Range<usize>) -> Dom<T> {

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
                // Row numbers (vertical) - "1", "2", "3"
                (rows.start..rows.end.saturating_sub(1))
                .map(|row_idx|
                    NodeData::label(format!("{}", row_idx + 1))
                    .with_classes(vec![DomString::Static("__azul-native-table-row")])
                )
                .collect::<Dom<T>>()
                .with_class("__azul-native-table-row-numbers")
            )
        )
        .with_child(
            columns
            .map(|col_idx|
                // Column name
                Dom::new(NodeType::Div)
                .with_class("__azul-native-table-column")
                .with_child(Dom::label(column_name_from_number(col_idx)).with_class("__azul-native-table-column-name"))
                .with_child(
                    // row contents - if no content is given, they are simply empty
                    (rows.start..rows.end)
                    .map(|row_idx|
                        NodeData::new(
                            if let Some(data) = self.work_sheet.get(&col_idx).and_then(|col| col.get(&row_idx)) {
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

    pub fn set_cell<I: Into<String>>(&mut self, x: usize, y: usize, value: I) {
        self.work_sheet
            .entry(x)
            .or_insert_with(|| BTreeMap::new())
            .insert(y, value.into());
    }
}

impl<T> TableView<T> {

    #[inline]
    pub fn new(state: Ref<TableViewState>) -> Self {
        Self { state, .. Default::default() }
    }

    #[inline]
    pub fn with_state(self, state: Ref<TableViewState>) -> Self {
        Self { state, .. self }
    }

    #[inline]
    pub fn on_mouse_up(self, cb: DefaultCallback<T>) -> Self {
        Self { on_mouse_up: cb, .. self }
    }

    #[inline]
    pub fn dom(self) -> Dom<T> {
        let upcasted_table_view = self.state.upcast();
        Dom::iframe(Self::render_table_iframe_contents, upcasted_table_view.clone())
            .with_class("__azul-native-table-iframe")
            .with_default_callback(On::MouseUp, self.on_mouse_up, upcasted_table_view)
    }

    pub fn default_on_mouse_up(_info: DefaultCallbackInfo<T>) -> CallbackReturn {
        println!("table was clicked");
        DontRedraw
    }

    fn render_table_iframe_contents(info: IFrameCallbackInfo) -> IFrameCallbackReturn<T> {
        let table_view_state = info.state.downcast::<TableViewState>()?;
        let table_view_state = table_view_state.borrow();
        let logical_size = info.bounds.get_logical_size();
        let necessary_rows = (logical_size.height as f32 / table_view_state.row_height).ceil() as usize;
        let necessary_columns = (logical_size.width as f32 / table_view_state.column_width).ceil() as usize;
        Some(table_view_state.render(0..necessary_rows, 0..necessary_columns))
    }
}

impl<T> Into<Dom<T>> for TableView<T> {
    fn into(self) -> Dom<T> {
        self.dom()
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