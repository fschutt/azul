//! Table view

use std::{ops::Range, collections::BTreeMap};
use azul::{
    style::StyledDom,
    dom::{Dom, NodeData, NodeType},
    callbacks::{RefAny, IFrameCallbackInfo, IFrameCallbackReturn},
};

#[derive(Debug, Clone)]
pub struct TableView {
    state: TableViewState,
}

impl Default for TableView {
    fn default() -> Self {
        Self {
            state: TableViewState::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TableViewState {
    /// Width of the column in pixels
    pub column_width: f32,
    /// Height of the column in pixels
    pub row_height: f32,
    /// Optional selection
    pub selection: Option<TableCellSelection>,
    /// Currently edited cells
    pub edited_cells: BTreeMap<TableCell, String>,
}

impl Default for TableViewState {
    fn default() -> Self {
        Self {
            column_width: 100.0,
            row_height: 20.0,
            selection: None,
            edited_cells: BTreeMap::default(),
        }
    }
}

/// Represents the index of a single cell (row + column)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableCell {
    pub row: usize,
    pub column: usize,
}

/// Represents a selection of table cells from the top left to the bottom right cell
#[derive(Debug, Clone)]
pub struct TableCellSelection {
    pub from_top_left: TableCell,
    pub to_bottom_right: TableCell,
}

impl TableCellSelection {

    pub fn from(row: usize, column: usize) -> Self {
        Self {
            from_top_left: TableCell { row, column },
            to_bottom_right: TableCell { row, column },
        }
    }

    pub fn to(self, row: usize, column: usize) -> Self {
        Self { to_bottom_right: TableCell { row, column }, .. self }
    }

    pub fn number_of_rows_selected(&self) -> usize {
        let max_row = self.from_top_left.row.max(self.to_bottom_right.row);
        let min_row = self.from_top_left.row.min(self.to_bottom_right.row);
        if max_row < min_row { 0 } else { (max_row - min_row) + 1 }
    }
    pub fn number_of_columns_selected(&self) -> usize {
        let max_col = self.from_top_left.column.max(self.to_bottom_right.column);
        let min_col = self.from_top_left.column.min(self.to_bottom_right.column);
        if max_col < min_col { 0 } else { (max_col - min_col) + 1 }
    }
}

impl TableViewState {

    pub fn new() -> Self {
        TableViewState::default()
    }

    pub fn set_cell<I: Into<String>>(&mut self, cell: TableCell, value: I) {
        self.edited_cells.insert(cell, value.into());
    }

    pub fn get_cell_contents(&self, cell: &TableCell) -> Option<&String> {
        self.edited_cells.get(cell)
    }

    pub fn set_selection(&mut self, selection: Option<TableCellSelection>) {
        self.selection = selection;
    }

    /// Renders a cutout of the table from, horizontally from (col_start..col_end)
    /// and vertically from (row_start..row_end)
    pub fn render(&self, rows: Range<usize>, columns: Range<usize>) -> StyledDom {

        use azul::css::*;
        use azul::str::String as AzString;
        use std::time::Instant;

        let i_start = Instant::now();

        println!("ok1!");

        let font: AzString = "sans-serif".into();
        let sans_serif_font_family = StyleFontFamily { fonts: vec![font].into() };

        const COLOR_407C40: ColorU = ColorU { r: 64, g: 124, b: 64, a: 0 }; // green
        const COLOR_2D2D2D: ColorU = ColorU { r: 45, g: 45, b: 45, a: 0 };
        const COLOR_E6E6E6: ColorU = ColorU { r: 230, g: 230, b: 230, a: 0 };
        const COLOR_B5B5B5: ColorU = ColorU { r: 181, g: 181, b: 181, a: 0 };
        const COLOR_D1D1D1: ColorU = ColorU { r: 209, g: 209, b: 209, a: 0 };
        const COLOR_BLACK: ColorU = ColorU { r: 0, g: 0, b: 0, a: 0 };

        const SELECTED_CELL_BORDER_WIDTH: isize = 2;
        const SELECTED_CELL_BORDER_STYLE: BorderStyle = BorderStyle::Solid;
        const SELECTED_CELL_BORDER_COLOR: ColorU = COLOR_407C40;

        let shadow = BoxShadowPreDisplayItem {
            offset: [PixelValueNoPercent::zero(), PixelValueNoPercent::zero()],
            color: COLOR_BLACK,
            blur_radius: PixelValueNoPercent::const_px(3),
            spread_radius: PixelValueNoPercent::const_px(3),
            clip_mode: BoxShadowClipMode::Outset,
        };

        println!("ok2!");

        // Empty rectangle at the top left of the table
        let top_left_empty_rect = Dom::div()
        .with_inline_css(CssProperty::height(LayoutHeight::const_px(20)))
        .with_inline_css(CssProperty::background_content(StyleBackgroundContent::Color(COLOR_E6E6E6)))
        .with_inline_css(CssProperty::border_bottom_color(StyleBorderBottomColor { inner: COLOR_B5B5B5 }))
        .with_inline_css(CssProperty::border_right_color(StyleBorderRightColor { inner: COLOR_B5B5B5 }));

        println!("ok3!");

        // Row numbers (first column - laid out vertical) - "1", "2", "3"
        let row_numbers = (rows.start..rows.end.saturating_sub(1)).map(|row_idx| {
            NodeData::label(format!("{}", row_idx + 1).into())
            .with_inline_css(CssProperty::font_size(StyleFontSize::const_px(14)))
            .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Row))
            .with_inline_css(CssProperty::justify_content(LayoutJustifyContent::Center))
            .with_inline_css(CssProperty::align_items(LayoutAlignItems::Center))
            .with_inline_css(CssProperty::min_height(LayoutMinHeight::const_px(20)))
            .with_inline_css(CssProperty::border_bottom_width(StyleBorderBottomWidth::const_px(1)))
            .with_inline_css(CssProperty::border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid }))
            .with_inline_css(CssProperty::border_bottom_color(StyleBorderBottomColor { inner: COLOR_B5B5B5 }))
        })
        .collect::<Dom>()
        .with_inline_css(CssProperty::font_family(sans_serif_font_family.clone()))
        .with_inline_css(CssProperty::text_color(StyleTextColor { inner: COLOR_2D2D2D }))
        .with_inline_css(CssProperty::background_content(StyleBackgroundContent::Color(COLOR_E6E6E6)))
        .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Column))
        .with_inline_css(CssProperty::box_shadow_right(shadow));

        println!("ok4!");

        // first column: contains the "top left rect" + the column
        let row_number_wrapper = Dom::div()
        .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Column))
        .with_inline_css(CssProperty::max_width(LayoutMaxWidth::px(30.0)))
        .with_child(top_left_empty_rect)
        .with_child(row_numbers);

        println!("ok5!");

        // currently active cell handle
        let current_active_selection_handle = Dom::div()
        .with_inline_css(CssProperty::position(LayoutPosition::Absolute))
        .with_inline_css(CssProperty::width(LayoutWidth::px(10.0)))
        .with_inline_css(CssProperty::height(LayoutHeight::px(10.0)))
        .with_inline_css(CssProperty::background_content(StyleBackgroundContent::Color(COLOR_407C40)))
        .with_inline_css(CssProperty::bottom(LayoutBottom::px(-5.0)))
        .with_inline_css(CssProperty::right(LayoutRight::px(-5.0))); // TODO: add callbacks to modify selection

        // currently selected cell(s)
        let current_active_selection = Dom::div()
        .with_inline_css(CssProperty::position(LayoutPosition::Absolute))
        .with_inline_css(CssProperty::width({
            self.selection.as_ref()
            .map(|selection| LayoutWidth::px(selection.number_of_columns_selected() as f32 * self.column_width))
            .unwrap_or(LayoutWidth::zero()) // TODO: replace with transform: scale-x
        }))
        .with_inline_css(CssProperty::height({
            self.selection.as_ref()
            .map(|selection| LayoutHeight::px(selection.number_of_rows_selected() as f32 * self.row_height))
            .unwrap_or(LayoutHeight::zero())  // TODO: replace with transform: scale-y
        }))
        .with_inline_css(CssProperty::margin_left({
            self.selection.as_ref()
            .map(|selection| LayoutMarginLeft::px(selection.from_top_left.column as f32 * self.column_width))
            .unwrap_or(LayoutMarginLeft::zero()) // TODO: replace with transform-y
        }))
        .with_inline_css(CssProperty::margin_top({
            self.selection.as_ref()
            .map(|selection| LayoutMarginTop::px(selection.from_top_left.row as f32 * self.row_height))
            .unwrap_or(LayoutMarginTop::zero()) // TODO: replace with transform-y
        }))

        .with_inline_css(CssProperty::border_bottom_width(StyleBorderBottomWidth::const_px(SELECTED_CELL_BORDER_WIDTH)))
        .with_inline_css(CssProperty::border_bottom_style(StyleBorderBottomStyle { inner: SELECTED_CELL_BORDER_STYLE }))
        .with_inline_css(CssProperty::border_bottom_color(StyleBorderBottomColor { inner: SELECTED_CELL_BORDER_COLOR }))
        .with_inline_css(CssProperty::border_top_width(StyleBorderTopWidth::const_px(SELECTED_CELL_BORDER_WIDTH)))
        .with_inline_css(CssProperty::border_top_style(StyleBorderTopStyle { inner: SELECTED_CELL_BORDER_STYLE }))
        .with_inline_css(CssProperty::border_top_color(StyleBorderTopColor { inner: SELECTED_CELL_BORDER_COLOR }))
        .with_inline_css(CssProperty::border_left_width(StyleBorderLeftWidth::const_px(SELECTED_CELL_BORDER_WIDTH)))
        .with_inline_css(CssProperty::border_left_style(StyleBorderLeftStyle { inner: SELECTED_CELL_BORDER_STYLE }))
        .with_inline_css(CssProperty::border_left_color(StyleBorderLeftColor { inner: SELECTED_CELL_BORDER_COLOR }))
        .with_inline_css(CssProperty::border_right_width(StyleBorderRightWidth::const_px(SELECTED_CELL_BORDER_WIDTH)))
        .with_inline_css(CssProperty::border_right_style(StyleBorderRightStyle { inner: SELECTED_CELL_BORDER_STYLE }))
        .with_inline_css(CssProperty::border_right_color(StyleBorderRightColor { inner: SELECTED_CELL_BORDER_COLOR }))

        // don't show the selection when the table doesn't have one
        // TODO: animate / fade in / fade out
        .with_inline_css(CssProperty::opacity(if self.selection.is_some() { StyleOpacity::const_new(1) } else { StyleOpacity::const_new(0) }))

        .with_child(current_active_selection_handle);

        println!("ok6!");

        let columns_table_container = columns.map(|col_idx| {

            let column_names = Dom::label(column_name_from_number(col_idx).into())
            .with_inline_css(CssProperty::height(LayoutHeight::px(20.0)))
            .with_inline_css(CssProperty::font_family(sans_serif_font_family.clone()))
            .with_inline_css(CssProperty::text_color(StyleTextColor { inner: COLOR_2D2D2D }))
            .with_inline_css(CssProperty::font_size(StyleFontSize::px(14.0)))
            .with_inline_css(CssProperty::background_content(StyleBackgroundContent::Color(COLOR_E6E6E6)))
            .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Row))
            .with_inline_css(CssProperty::align_items(LayoutAlignItems::Center))
            .with_inline_css(CssProperty::border_right_width(StyleBorderRightWidth::const_px(1)))
            .with_inline_css(CssProperty::border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid }))
            .with_inline_css(CssProperty::border_right_color(StyleBorderRightColor { inner: COLOR_B5B5B5 }))
            .with_inline_css(CssProperty::box_shadow_bottom(shadow));


            // rows in this column, laid out vertically
            let rows_in_this_column = (rows.start..rows.end)
                .map(|row_idx| {

                    let node_type = match self.get_cell_contents(&TableCell { row: row_idx, column: col_idx }) {
                        Some(string) => NodeType::Label(string.as_str().into()),
                        None => NodeType::Label("".into()),
                    };

                    NodeData::new(node_type)
                    .with_inline_css(CssProperty::align_items(LayoutAlignItems::FlexStart))
                    .with_inline_css(CssProperty::height(LayoutHeight::px(20.0)))
                    .with_inline_css(CssProperty::font_size(StyleFontSize::px(14.0)))
                    .with_inline_css(CssProperty::text_align(StyleTextAlignmentHorz::Left))
                    .with_inline_css(CssProperty::text_color(StyleTextColor { inner: COLOR_BLACK }))
                    .with_inline_css(CssProperty::font_family(sans_serif_font_family.clone()))
                    .with_inline_css(CssProperty::border_bottom_width(StyleBorderBottomWidth::px(1.0)))
                    .with_inline_css(CssProperty::border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid }))
                    .with_inline_css(CssProperty::border_bottom_color(StyleBorderBottomColor { inner: COLOR_D1D1D1 }))
                })
                .collect::<Dom>();

            // Column name
            Dom::div()
                .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Column))
                .with_inline_css(CssProperty::min_width(LayoutMinWidth::px(100.0)))
                .with_inline_css(CssProperty::border_right_width(StyleBorderRightWidth::px(1.0)))
                .with_inline_css(CssProperty::border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid }))
                .with_inline_css(CssProperty::border_right_color(StyleBorderRightColor { inner: COLOR_D1D1D1 }))
                .with_child(column_names)
                .with_child(rows_in_this_column)
        })
        .collect::<Dom>()
        .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Row))
        .with_inline_css(CssProperty::position(LayoutPosition::Relative));


        println!("current_active_selection is now: {:#?}", current_active_selection);
        println!("dom is now: {:#?}", columns_table_container);

        let columns_table_container =  columns_table_container
        .with_child(current_active_selection);

        println!("ok7!");

        let dom = Dom::div()
        .with_inline_css(CssProperty::display(LayoutDisplay::Flex))
        .with_inline_css(CssProperty::box_sizing(LayoutBoxSizing::BorderBox))
        .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Row))
            .with_child(row_number_wrapper)
            .with_child(columns_table_container);

        println!("ok: {:?}!", dom);

        let styled = dom.style(Css::empty());

        println!("ok: styled dom in {:?}", Instant::now() - i_start);

        styled
    }
}

impl TableView {

    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn with_state(self, state: TableViewState) -> Self {
        Self { state, .. self }
    }

    #[inline]
    pub fn dom(self) -> StyledDom {

        use azul::css::*;

        let state_ref = RefAny::new(self.state);

        Dom::iframe(state_ref, Self::render_table_iframe_contents)
            .with_inline_css(CssProperty::display(LayoutDisplay::Flex))
            .with_inline_css(CssProperty::flex_grow(LayoutFlexGrow::const_new(1)))
            .with_inline_css(CssProperty::width(LayoutWidth::const_percent(100)))
            .with_inline_css(CssProperty::height(LayoutHeight::const_percent(100)))
            .with_inline_css(CssProperty::box_sizing(LayoutBoxSizing::BorderBox))
            .style(Css::empty())
    }

    extern "C" fn render_table_iframe_contents(state: &RefAny, info: IFrameCallbackInfo) -> IFrameCallbackReturn {

        use azul::window::{LayoutRect, LayoutSize, LayoutPoint};

        println!("in function render_table_iframe_contents!");
        println!("state: {:?}", state);

        let table_view_state = state.borrow::<TableViewState>().unwrap();
        println!("downcast worked!");
        let logical_size = info.get_bounds().get_logical_size();
        println!("info get bounds: {:?}!", logical_size);

        let padding_rows = 0;
        let padding_columns = 0;
        let row_start = 0; // bounds.top / table_view_state.row_height
        let column_start = 0; // bounds.left / table_view_state.column_width

        let necessary_rows = (logical_size.height as f32 / table_view_state.row_height).ceil() as usize;
        let necessary_columns = (logical_size.width as f32 / table_view_state.column_width).ceil() as usize;

        let table_height = (necessary_rows + padding_rows) as f32 * table_view_state.row_height;
        let table_width = (necessary_columns + padding_columns) as f32 * table_view_state.column_width;

        println!("calling table view state render!");
        let styled_dom = table_view_state.render(
            row_start..(row_start + necessary_rows + padding_rows),
            column_start..(column_start + necessary_columns + padding_columns)
        );

        println!("styled dom rendered: {:#?}", styled_dom);

        IFrameCallbackReturn {
            dom: styled_dom,
            size: LayoutRect {
                origin: LayoutPoint::zero(), // TODO: info.get_bounds().origin,
                size: LayoutSize::new(table_width.floor() as isize, table_height.floor() as isize),
            },
            virtual_size: None.into(),
        }
    }
}

impl From<TableView> for StyledDom  {
    fn from(t: TableView) -> StyledDom {
        t.dom()
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