//! Table view

use core::ops::Range;
use alloc::collections::BTreeMap;
use alloc::string::String;
use azul::{
    style::StyledDom,
    dom::{Dom, NodeData, NodeType},
    vec::CssPropertyVec,
    callbacks::UpdateScreen,
    callbacks::{RefAny, IFrameCallbackInfo, IFrameCallbackReturn},
};

pub type RowIndex = usize;
pub type ColumnIndex = usize;

#[derive(Debug, Clone)]
pub struct TableView {
    pub state: TableViewState,
    // pub disable_selection: bool,
    // pub on_cell_focus_received: Option<(OnCellEditFinishCallback, RefAny)>
    // pub on_cell_focus_lost: Option<(OnCellEditFinishCallback, RefAny)>
    // pub on_cell_edit_finish: Option<(OnCellEditFinishCallback, RefAny)>
}

impl Default for TableView {
    fn default() -> Self {
        Self {
            state: TableViewState::default(),
        }
    }
}

pub type OnCellFocusReceivedCallback = extern "C" fn(&mut RefAny, &TableViewState, TableCellIndex) -> UpdateScreen;
pub type OnCellFocusLostCallback = extern "C" fn(&mut RefAny, &TableViewState, TableCellIndex) -> UpdateScreen;
pub type OnCellEditFinishCallback = extern "C" fn(&mut RefAny, &TableViewState, TableCellIndex) -> UpdateScreen;
pub type OnCellInputCallback = extern "C" fn(&mut RefAny, &TableViewState, TableCellIndex) -> UpdateScreen;

#[derive(Debug, Default, Clone)]
pub struct TableStyle {
    // TODO: styling args (background / border, etc.)
}

#[derive(Debug, Clone)]
pub struct TableViewState {
    pub style: TableStyle,
    /// Width of the column in pixels
    pub default_column_width: f32,
    /// Height of the column in pixels
    pub default_row_height: f32,
    /// Overrides the `default_column_width` for column X
    pub column_width_overrides: BTreeMap<ColumnIndex, f32>,
    /// Overrides the `default_row_height` for row X
    pub row_height_overrides: BTreeMap<RowIndex, f32>,
    /// Optional selection
    pub selection: Option<TableCellSelection>,
    /// Current cell contents
    pub cell_contents: BTreeMap<TableCellIndex, String>,
}

impl Default for TableViewState {
    fn default() -> Self {
        Self {
            style: TableStyle::default(),
            default_column_width: 100.0,
            default_row_height: 20.0,
            column_width_overrides: BTreeMap::default(),
            row_height_overrides: BTreeMap::default(),
            selection: None,
            cell_contents: BTreeMap::default(),
        }
    }
}

/// Represents the index of a single cell (row + column)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableCellIndex {
    pub row: RowIndex,
    pub column: ColumnIndex,
}

/// Represents a selection of table cells from the top left to the bottom right cell
#[derive(Debug, Clone)]
pub struct TableCellSelection {
    pub from_top_left: TableCellIndex,
    pub to_bottom_right: TableCellIndex,
}

impl TableCellSelection {

    pub fn from(row: usize, column: usize) -> Self {
        Self {
            from_top_left: TableCellIndex { row, column },
            to_bottom_right: TableCellIndex { row, column },
        }
    }

    pub fn to(self, row: usize, column: usize) -> Self {
        Self { to_bottom_right: TableCellIndex { row, column }, .. self }
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

    pub fn set_cell_content<I: Into<String>>(&mut self, cell: TableCellIndex, value: I) {
        self.cell_contents.insert(cell, value.into());
    }

    pub fn get_cell_content(&self, cell: &TableCellIndex) -> Option<&String> {
        self.cell_contents.get(cell)
    }

    pub fn set_selection(&mut self, selection: Option<TableCellSelection>) {
        self.selection = selection;
    }

    /// Renders a cutout of the table from, horizontally from (col_start..col_end)
    /// and vertically from (row_start..row_end)
    pub fn render(&self, rows: Range<usize>, columns: Range<usize>) -> StyledDom {

        use azul::css::*;
        use azul::str::String as AzString;
        use azul::vec::StringVec as AzStringVec;

        let font: AzString = "sans-serif".into();
        let font_vec: AzStringVec = [font][..].into();
        let sans_serif_font_family = StyleFontFamily { fonts: font_vec };

        const COLOR_407C40: ColorU = ColorU { r: 64, g: 124, b: 64, a: 255 }; // green
        const COLOR_2D2D2D: ColorU = ColorU { r: 45, g: 45, b: 45, a: 255 };
        const COLOR_E6E6E6: ColorU = ColorU { r: 230, g: 230, b: 230, a: 255 };
        const COLOR_B5B5B5: ColorU = ColorU { r: 181, g: 181, b: 181, a: 255 };
        const COLOR_D1D1D1: ColorU = ColorU { r: 209, g: 209, b: 209, a: 255 };
        const COLOR_BLACK: ColorU = ColorU { r: 0, g: 0, b: 0, a: 255 };

        const SELECTED_CELL_BORDER_WIDTH: isize = 2;
        const SELECTED_CELL_BORDER_STYLE: BorderStyle = BorderStyle::Solid;
        const SELECTED_CELL_BORDER_COLOR: ColorU = COLOR_407C40;

        let shadow = BoxShadowPreDisplayItem {
            offset: [PixelValueNoPercent::zero(), PixelValueNoPercent::zero()],
            color: COLOR_2D2D2D,
            blur_radius: PixelValueNoPercent::const_px(3),
            spread_radius: PixelValueNoPercent::const_px(3),
            clip_mode: BoxShadowClipMode::Outset,
        };

        // Empty rectangle at the top left of the table
        let top_left_empty_rect = Dom::div()
        .with_inline_css_props(CssPropertyVec::from(&[
            CssProperty::height(LayoutHeight::const_px(20)),
            CssProperty::background_content(StyleBackgroundContent::Color(COLOR_E6E6E6)),
            CssProperty::border_bottom_color(StyleBorderBottomColor { inner: COLOR_B5B5B5 }),
            CssProperty::border_right_color(StyleBorderRightColor { inner: COLOR_B5B5B5 }),
        ][..]));

        // Row numbers (first column - laid out vertical) - "1", "2", "3"
        let row_numbers = (rows.start..rows.end.saturating_sub(1)).map(|row_idx| {

            use crate::alloc::string::ToString;

            // NOTE: to_string() heap allocation is unavoidable

            NodeData::label((row_idx + 1).to_string().into())
            .with_inline_css_props(CssPropertyVec::from(&[
                CssProperty::font_size(StyleFontSize::const_px(14)),
                CssProperty::flex_direction(LayoutFlexDirection::Row),
                CssProperty::justify_content(LayoutJustifyContent::Center),
                CssProperty::align_items(LayoutAlignItems::Center),
                CssProperty::min_height(LayoutMinHeight::const_px(20)),
                CssProperty::border_bottom_width(StyleBorderBottomWidth::const_px(1)),
                CssProperty::border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid }),
                CssProperty::border_bottom_color(StyleBorderBottomColor { inner: COLOR_B5B5B5 }),
            ][..]))
        })
        .collect::<Dom>()
        .with_inline_css_props(CssPropertyVec::from(&[
            CssProperty::font_family(sans_serif_font_family.clone()),
            CssProperty::text_color(StyleTextColor { inner: COLOR_2D2D2D }),
            CssProperty::background_content(StyleBackgroundContent::Color(COLOR_E6E6E6)),
            CssProperty::flex_direction(LayoutFlexDirection::Column),
            CssProperty::box_shadow_right(shadow),
        ][..]));

        // first column: contains the "top left rect" + the column
        let row_number_wrapper = Dom::div()
        .with_inline_css_props(CssPropertyVec::from(&[
            CssProperty::flex_direction(LayoutFlexDirection::Column),
            CssProperty::max_width(LayoutMaxWidth::px(30.0)),
        ][..]))
        .with_child(top_left_empty_rect)
        .with_child(row_numbers);

        // currently active cell handle
        // TODO: add callbacks to modify selection
        let current_active_selection_handle = Dom::div()
        .with_inline_css_props(CssPropertyVec::from(&[
            CssProperty::position(LayoutPosition::Absolute),
            CssProperty::width(LayoutWidth::px(10.0)),
            CssProperty::height(LayoutHeight::px(10.0)),
            CssProperty::background_content(StyleBackgroundContent::Color(COLOR_407C40)),
            CssProperty::bottom(LayoutBottom::px(-5.0)),
            CssProperty::right(LayoutRight::px(-5.0)),
        ][..]));

        // currently selected cell(s)
        let current_active_selection = Dom::div()
        .with_inline_css_props(CssPropertyVec::from(&[
            CssProperty::position(LayoutPosition::Absolute),
            CssProperty::width({
                self.selection.as_ref()
                .map(|selection| LayoutWidth::px(selection.number_of_columns_selected() as f32 * self.default_column_width))
                .unwrap_or(LayoutWidth::zero()) // TODO: replace with transform: scale-x
            }),
            CssProperty::height({
                self.selection.as_ref()
                .map(|selection| LayoutHeight::px(selection.number_of_rows_selected() as f32 * self.default_row_height))
                .unwrap_or(LayoutHeight::zero())  // TODO: replace with transform: scale-y
            }),
            CssProperty::margin_left({
                self.selection.as_ref()
                .map(|selection| LayoutMarginLeft::px(selection.from_top_left.column as f32 * self.default_column_width))
                .unwrap_or(LayoutMarginLeft::zero()) // TODO: replace with transform-y
            }),
            CssProperty::margin_top({
                self.selection.as_ref()
                .map(|selection| LayoutMarginTop::px(selection.from_top_left.row as f32 * self.default_row_height))
                .unwrap_or(LayoutMarginTop::zero()) // TODO: replace with transform-y
            }),
            CssProperty::border_bottom_width(StyleBorderBottomWidth::const_px(SELECTED_CELL_BORDER_WIDTH)),
            CssProperty::border_bottom_style(StyleBorderBottomStyle { inner: SELECTED_CELL_BORDER_STYLE }),
            CssProperty::border_bottom_color(StyleBorderBottomColor { inner: SELECTED_CELL_BORDER_COLOR }),
            CssProperty::border_top_width(StyleBorderTopWidth::const_px(SELECTED_CELL_BORDER_WIDTH)),
            CssProperty::border_top_style(StyleBorderTopStyle { inner: SELECTED_CELL_BORDER_STYLE }),
            CssProperty::border_top_color(StyleBorderTopColor { inner: SELECTED_CELL_BORDER_COLOR }),
            CssProperty::border_left_width(StyleBorderLeftWidth::const_px(SELECTED_CELL_BORDER_WIDTH)),
            CssProperty::border_left_style(StyleBorderLeftStyle { inner: SELECTED_CELL_BORDER_STYLE }),
            CssProperty::border_left_color(StyleBorderLeftColor { inner: SELECTED_CELL_BORDER_COLOR }),
            CssProperty::border_right_width(StyleBorderRightWidth::const_px(SELECTED_CELL_BORDER_WIDTH)),
            CssProperty::border_right_style(StyleBorderRightStyle { inner: SELECTED_CELL_BORDER_STYLE }),
            CssProperty::border_right_color(StyleBorderRightColor { inner: SELECTED_CELL_BORDER_COLOR }),
            // don't show the selection when the table doesn't have one
            // TODO: animate / fade in / fade out
            CssProperty::opacity(if self.selection.is_some() { StyleOpacity::const_new(1) } else { StyleOpacity::const_new(0) }),
        ][..]))
        .with_child(current_active_selection_handle);

        let columns_table_container = columns.map(|col_idx| {

            // avoid heap allocation
            let mut column_name_arr = [0;16];
            let zeroed_characters = column_name_from_number(col_idx, &mut column_name_arr);
            let slice = &column_name_arr[zeroed_characters..];
            let s = unsafe { ::core::str::from_utf8_unchecked(slice) };

            let column_names = Dom::label(s.into())
            .with_inline_css_props(CssPropertyVec::from(&[
                CssProperty::height(LayoutHeight::px(20.0)),
                CssProperty::font_family(sans_serif_font_family.clone()),
                CssProperty::text_color(StyleTextColor { inner: COLOR_2D2D2D }),
                CssProperty::font_size(StyleFontSize::px(14.0)),
                CssProperty::background_content(StyleBackgroundContent::Color(COLOR_E6E6E6)),
                CssProperty::flex_direction(LayoutFlexDirection::Row),
                CssProperty::align_items(LayoutAlignItems::Center),
                CssProperty::border_right_width(StyleBorderRightWidth::const_px(1)),
                CssProperty::border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid }),
                CssProperty::border_right_color(StyleBorderRightColor { inner: COLOR_B5B5B5 }),
                CssProperty::box_shadow_bottom(shadow),
            ][..]));


            // rows in this column, laid out vertically
            let rows_in_this_column = (rows.start..rows.end)
                .map(|row_idx| {

                    let node_type = match self.get_cell_content(&TableCellIndex { row: row_idx, column: col_idx }) {
                        Some(string) => NodeType::Label(string.as_str().into()),
                        None => NodeType::Label("".into()),
                    };

                    NodeData::new(node_type)
                    .with_inline_css_props(CssPropertyVec::from(&[
                       CssProperty::align_items(LayoutAlignItems::FlexStart),
                       CssProperty::height(LayoutHeight::px(20.0)),
                       CssProperty::font_size(StyleFontSize::px(14.0)),
                       CssProperty::text_align(StyleTextAlignmentHorz::Left),
                       CssProperty::text_color(StyleTextColor { inner: COLOR_BLACK }),
                       CssProperty::font_family(sans_serif_font_family.clone()),
                       CssProperty::border_bottom_width(StyleBorderBottomWidth::px(1.0)),
                       CssProperty::border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid }),
                       CssProperty::border_bottom_color(StyleBorderBottomColor { inner: COLOR_D1D1D1 }),
                    ][..]))
                })
                .collect::<Dom>();

            // Column name
            Dom::div()
            .with_inline_css_props(CssPropertyVec::from(&[
                CssProperty::flex_direction(LayoutFlexDirection::Column),
                CssProperty::min_width(LayoutMinWidth::px(100.0)),
                CssProperty::border_right_width(StyleBorderRightWidth::px(1.0)),
                CssProperty::border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid }),
                CssProperty::border_right_color(StyleBorderRightColor { inner: COLOR_D1D1D1 }),
            ][..]))
            .with_child(column_names)
            .with_child(rows_in_this_column)
        })
        .collect::<Dom>()
        .with_inline_css_props(CssPropertyVec::from(&[
            CssProperty::flex_direction(LayoutFlexDirection::Row),
            CssProperty::position(LayoutPosition::Relative),
        ][..]));

        let columns_table_container =  columns_table_container
        .with_child(current_active_selection);

        let dom = Dom::div()
        .with_inline_css_props(CssPropertyVec::from(&[
            CssProperty::display(LayoutDisplay::Flex),
            CssProperty::box_sizing(LayoutBoxSizing::BorderBox),
            CssProperty::flex_direction(LayoutFlexDirection::Row),
        ][..]))
        .with_child(row_number_wrapper)
        .with_child(columns_table_container);

        let styled = dom.style(Css::empty());

        styled
    }
}

impl TableView {

    #[inline]
    pub const fn new(state: TableViewState) -> Self {
        Self { state }
    }

    #[inline]
    pub fn dom(self) -> StyledDom {

        use azul::css::*;

        Dom::iframe(RefAny::new(self.state), Self::render_table_iframe_contents)
        .with_inline_css_props(CssPropertyVec::from(&[
           CssProperty::display(LayoutDisplay::Flex),
           CssProperty::flex_grow(LayoutFlexGrow::const_new(1)),
           CssProperty::width(LayoutWidth::const_percent(100)),
           CssProperty::height(LayoutHeight::const_percent(100)),
           CssProperty::box_sizing(LayoutBoxSizing::BorderBox),
        ][..]))
        .style(Css::empty())
    }

    extern "C" fn render_table_iframe_contents(state: &RefAny, info: IFrameCallbackInfo) -> IFrameCallbackReturn {

        use azul::window::{LayoutRect, LayoutSize, LayoutPoint};

        let table_view_state = state.downcast_ref::<TableViewState>().unwrap();
        let logical_size = info.get_bounds().get_logical_size();
        let padding_rows = 0;
        let padding_columns = 0;
        let row_start = 0; // bounds.top / table_view_state.row_height
        let column_start = 0; // bounds.left / table_view_state.column_width

        // workaround for necessary_rows.ceil() not being available on no_std
        let necessary_rows_f32 = logical_size.height as f32 / table_view_state.default_row_height;
        let necessary_rows = if (necessary_rows_f32 * 10.0) as isize % 10_isize != 0 { necessary_rows_f32 as usize + 1 } else { necessary_rows_f32 as usize };
        let necessary_columns_f32 = logical_size.width as f32 / table_view_state.default_column_width;
        let necessary_columns = if (necessary_columns_f32 * 10.0) as isize % 10_isize != 0 { necessary_columns_f32 as usize + 1 } else { necessary_columns_f32 as usize };

        let table_height = (necessary_rows + padding_rows) as f32 * table_view_state.default_row_height;
        let table_width = (necessary_columns + padding_columns) as f32 * table_view_state.default_column_width;

        let styled_dom = table_view_state.render(
            row_start..(row_start + necessary_rows + padding_rows),
            column_start..(column_start + necessary_columns + padding_columns)
        );

        IFrameCallbackReturn {
            dom: styled_dom,
            size: LayoutRect {
                origin: LayoutPoint::zero(), // TODO: info.get_bounds().origin,
                size: LayoutSize::new(table_width as isize, table_height as isize),
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

const ALPHABET_LEN: usize = 26;
// usize::MAX is "GKGWBYLWRXTLPP" with a length of 15 characters
const MAX_LEN: usize = 15;

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
pub fn column_name_from_number(num: usize, result: &mut [u8; 16]) -> usize {

    #[inline(always)]
    fn u8_to_char(input: u8) -> u8 {
        'A' as u8 + input
    }

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
    zeroed_characters
}

pub fn char_less_than_10_from_digit(num: u32) -> Option<char> {
    let radix = 10_u32;
    if num < radix {
        let num = num as u8;
        if num < 10 {
            Some((b'0' + num) as char)
        } else {
            Some((b'a' + num - 10) as char)
        }
    } else {
        None
    }
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