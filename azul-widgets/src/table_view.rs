//! Table view

use std::{ops::Range, collections::BTreeMap};
use azul::{
    dom::{Dom, On, NodeData, NodeType},
    callbacks::{
        RefAny, Callback, CallbackInfo, CallbackReturn,
        IFrameCallbackInfo, IFrameCallbackReturn, UpdateScreen,
    },
};

#[derive(Debug, Clone)]
pub struct TableView {
    state: RefAny, // Ref<TableViewState>,
    on_mouse_up: Callback,
}

impl Default for TableView {
    fn default() -> Self {
        Self {
            state: RefAny::new(TableViewState::default()),
            on_mouse_up: Callback { cb: Self::default_on_mouse_up },
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
    pub fn render(&self, rows: Range<usize>, columns: Range<usize>) -> Dom {

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
        .with_class("__azul-native-table-container".into())
        .with_child(
            Dom::div()
            .with_class("__azul-native-table-row-number-wrapper".into())
            .with_child(
                // Empty rectangle at the top left of the table
                Dom::div()
                .with_class("__azul-native-table-top-left-rect".into())
            )
            .with_child(
                // Row numbers (vertical) - "1", "2", "3"
                (rows.start..rows.end.saturating_sub(1))
                .map(|row_idx|
                    NodeData::label(format!("{}", row_idx + 1))
                    .with_classes(vec!["__azul-native-table-row".into()])
                )
                .collect::<Dom>()
                .with_class("__azul-native-table-row-numbers".into())
            )
        )
        .with_child(
            columns
            .map(|col_idx|
                // Column name
                Dom::new(NodeType::Div)
                .with_class("__azul-native-table-column".into())
                .with_child(Dom::label(column_name_from_number(col_idx)).with_class("__azul-native-table-column-name".into()))
                .with_child(
                    // row contents - if no content is given, they are simply empty
                    (rows.start..rows.end)
                    .map(|row_idx|
                        NodeData::new(
                            if let Some(data) = self.work_sheet.get(&col_idx).and_then(|col| col.get(&row_idx)) {
                                NodeType::Label(data.into())
                            } else {
                                NodeType::Div
                            }
                        ).with_classes(vec!["__azul-native-table-cell".into()])
                    )
                    .collect::<Dom>()
                    .with_class("__azul-native-table-rows".into())
                )
            )
            .collect::<Dom>()
            .with_class("__azul-native-table-column-container".into())
            // current active selection (s)
            .with_child(
                Dom::div()
                    .with_class("__azul-native-table-selection".into())
                    .with_child(Dom::div().with_class("__azul-native-table-selection-handle".into()))
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

impl TableView {

    #[inline]
    pub fn new(state: TableViewState) -> Self {
        Self { state: RefAny::new(state), .. Default::default() }
    }

    #[inline]
    pub fn with_state(self, state: TableViewState) -> Self {
        Self { state: RefAny::new(state), .. self }
    }

    #[inline]
    pub fn on_mouse_up(self, cb: Callback) -> Self {
        Self { on_mouse_up: cb, .. self }
    }

    #[inline]
    pub fn dom(self) -> Dom {
        Dom::iframe(self.state.clone(), Self::render_table_iframe_contents)
            .with_class("__azul-native-table-iframe".into())
            .with_callback(On::MouseUp, self.state, self.on_mouse_up.cb)
    }

    pub fn default_on_mouse_up(_info: CallbackInfo) -> CallbackReturn {
        println!("table was clicked");
        UpdateScreen::DontRedraw
    }

    fn render_table_iframe_contents(info: IFrameCallbackInfo) -> IFrameCallbackReturn {
        fn render_table_iframe_contents_inner(info: IFrameCallbackInfo) -> Option<Dom> {
            let table_view_state = info.state().borrow::<TableViewState>()?;
            let logical_size = info.bounds.get_logical_size();
            let necessary_rows = (logical_size.height as f32 / table_view_state.row_height).ceil() as usize;
            let necessary_columns = (logical_size.width as f32 / table_view_state.column_width).ceil() as usize;
            Some(table_view_state.render(0..necessary_rows, 0..necessary_columns)).into()
        }
        IFrameCallbackReturn { dom: render_table_iframe_contents_inner(info).into() }
    }
}

impl Into<Dom> for TableView {
    fn into(self) -> Dom {
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