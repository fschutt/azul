//! Table view

use core::ops::Range;
use alloc::collections::BTreeMap;
use alloc::string::String;
use azul::{
    style::StyledDom,
    dom::{Dom, NodeData, NodeType},
    callbacks::Update,
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

pub type OnCellFocusReceivedCallback = extern "C" fn(&mut RefAny, &TableViewState, TableCellIndex) -> Update;
pub type OnCellFocusLostCallback = extern "C" fn(&mut RefAny, &TableViewState, TableCellIndex) -> Update;
pub type OnCellEditFinishCallback = extern "C" fn(&mut RefAny, &TableViewState, TableCellIndex) -> Update;
pub type OnCellInputCallback = extern "C" fn(&mut RefAny, &TableViewState, TableCellIndex) -> Update;

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
    #[inline]
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

    #[inline]
    pub fn from(row: usize, column: usize) -> Self {
        Self {
            from_top_left: TableCellIndex { row, column },
            to_bottom_right: TableCellIndex { row, column },
        }
    }

    #[inline]
    pub fn to(self, row: usize, column: usize) -> Self {
        Self { to_bottom_right: TableCellIndex { row, column }, .. self }
    }

    #[inline]
    pub fn number_of_rows_selected(&self) -> usize {
        let max_row = self.from_top_left.row.max(self.to_bottom_right.row);
        let min_row = self.from_top_left.row.min(self.to_bottom_right.row);
        if max_row < min_row { 0 } else { (max_row - min_row) + 1 }
    }

    #[inline]
    pub fn number_of_columns_selected(&self) -> usize {
        let max_col = self.from_top_left.column.max(self.to_bottom_right.column);
        let min_col = self.from_top_left.column.min(self.to_bottom_right.column);
        if max_col < min_col { 0 } else { (max_col - min_col) + 1 }
    }
}

impl TableViewState {

    #[inline]
    pub fn new() -> Self {
        TableViewState::default()
    }

    #[inline]
    pub fn set_cell_content<I: Into<String>>(&mut self, cell: TableCellIndex, value: I) {
        self.cell_contents.insert(cell, value.into());
    }

    #[inline]
    pub fn get_cell_content(&self, cell: &TableCellIndex) -> Option<&String> {
        self.cell_contents.get(cell)
    }

    #[inline]
    pub fn set_selection(&mut self, selection: Option<TableCellSelection>) {
        self.selection = selection;
    }

    /// Renders a cutout of the table from, horizontally from (col_start..col_end)
    /// and vertically from (row_start..row_end)
    pub fn dom(&self, rows: Range<usize>, columns: Range<usize>) -> Dom {

        use azul::css::*;
        use azul::str::String as AzString;
        use azul::vec::DomVec as AzDomVec;
        use azul::vec::StyleBackgroundContentVec;
        use azul::vec::NodeDataInlineCssPropertyVec;
        use azul::vec::StyleFontFamilyVec;
        use azul::vec::StyleTransformVec;
        use azul::vec::IdOrClassVec;
        use azul::dom::IdOrClass;
        use azul::dom::NodeDataInlineCssProperty;
        use azul::dom::NodeDataInlineCssProperty::Normal;

        const FONT_STRING: AzString = AzString::from_const_str("sans-serif");
        const FONT_VEC: &[StyleFontFamily] = &[StyleFontFamily::System(FONT_STRING)];
        const SANS_SERIF_FONT_FAMILY: StyleFontFamilyVec = StyleFontFamilyVec::from_const_slice(FONT_VEC);

        const COLOR_407C40: ColorU = ColorU { r: 64, g: 124, b: 64, a: 255 }; // green
        const COLOR_2D2D2D: ColorU = ColorU { r: 45, g: 45, b: 45, a: 255 };
        const COLOR_E6E6E6: ColorU = ColorU { r: 230, g: 230, b: 230, a: 255 };
        const COLOR_B5B5B5: ColorU = ColorU { r: 181, g: 181, b: 181, a: 255 };
        const COLOR_D1D1D1: ColorU = ColorU { r: 209, g: 209, b: 209, a: 255 };
        const COLOR_BLACK: ColorU = ColorU { r: 0, g: 0, b: 0, a: 255 };

        static COLOR_407C40_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(COLOR_407C40)];
        static COLOR_E6E6E6_BACKGROUND: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(COLOR_E6E6E6)];

        static SELECTED_CELL_BORDER_WIDTH: isize = 2;
        static SELECTED_CELL_BORDER_STYLE: BorderStyle = BorderStyle::Solid;
        static SELECTED_CELL_BORDER_COLOR: ColorU = COLOR_407C40;

        const DEFAULT_TABLE_CELL_STRING: AzString = AzString::from_const_str("");

        static SHADOW: StyleBoxShadow = StyleBoxShadow {
            offset: [PixelValueNoPercent::zero(), PixelValueNoPercent::zero()],
            color: COLOR_2D2D2D,
            blur_radius: PixelValueNoPercent::const_px(3),
            spread_radius: PixelValueNoPercent::const_px(3),
            clip_mode: BoxShadowClipMode::Outset,
        };

        static TOP_LEFT_EMPTY_RECT_STYLE: &[NodeDataInlineCssProperty] = &[
            Normal(CssProperty::const_min_height(LayoutMinHeight::const_px(20))),
            Normal(CssProperty::const_height(LayoutHeight::const_px(20))),
            Normal(CssProperty::const_max_height(LayoutMaxHeight::const_px(20))),
            Normal(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(COLOR_E6E6E6_BACKGROUND))),
            Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_B5B5B5 })),
            Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_B5B5B5 })),
        ];
        static TOP_LEFT_EMPTY_RECT_CLASS: &[IdOrClass] = &[
            IdOrClass::Class(AzString::from_const_str("az-table-top-left-rect"))
        ];

        // Empty rectangle at the top left of the table
        let top_left_empty_rect = Dom::div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(TOP_LEFT_EMPTY_RECT_CLASS))
        .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(TOP_LEFT_EMPTY_RECT_STYLE));

        static ROW_NUMBERS_CONTAINER_STYLE: &[NodeDataInlineCssProperty] = &[
            Normal(CssProperty::const_font_family(SANS_SERIF_FONT_FAMILY)),
            Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_2D2D2D })),
            Normal(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(COLOR_E6E6E6_BACKGROUND))),
            Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
            Normal(CssProperty::const_box_shadow_right(SHADOW)),
        ];
        static ROW_NUMBERS_CONTAINER_CLASS: &[IdOrClass] = &[
            IdOrClass::Class(AzString::from_const_str("az-table-row-numbers-container"))
        ];

        // Row numbers (first column - laid out vertical) - "1", "2", "3"
        let row_numbers = (rows.start..rows.end).map(|row_idx| {

            static ROW_NUMBERS_STYLE: &[NodeDataInlineCssProperty] = &[
                Normal(CssProperty::const_font_size(StyleFontSize::const_px(14))),
                Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
                Normal(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
                Normal(CssProperty::const_align_items(LayoutAlignItems::Center)),
                Normal(CssProperty::const_min_height(LayoutMinHeight::const_px(20))),
                Normal(CssProperty::const_height(LayoutHeight::const_px(20))),
                Normal(CssProperty::const_max_height(LayoutMaxHeight::const_px(20))),
                Normal(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
                Normal(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
                Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_B5B5B5 })),
            ];
            static ROW_NUMBERS_CLASS: &[IdOrClass] = &[
                IdOrClass::Class(AzString::from_const_str("az-table-row-numbers"))
            ];

            // NOTE: to_string() heap allocation is unavoidable
            NodeData::text((row_idx + 1).to_string().into())
            .with_ids_and_classes(IdOrClassVec::from_const_slice(ROW_NUMBERS_CLASS))
            .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(ROW_NUMBERS_STYLE))
        })
        .collect::<Dom>()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(ROW_NUMBERS_CONTAINER_CLASS))
        .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(ROW_NUMBERS_CONTAINER_STYLE));

        static ROW_NUMBER_WRAPPER_STYLE: &[NodeDataInlineCssProperty] = &[
            Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
            Normal(CssProperty::const_max_width(LayoutMaxWidth::const_px(30))),
            Normal(CssProperty::const_width(LayoutWidth::const_px(30))),
            Normal(CssProperty::const_min_width(LayoutMinWidth::const_px(30))),
        ];
        static ROW_NUMBERS_WRAPPER_CLASS: &[IdOrClass] = &[
            IdOrClass::Class(AzString::from_const_str("az-table-row-numbers-wrapper"))
        ];

        // first column: contains the "top left rect" + the column
        let row_number_wrapper = Dom::div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(ROW_NUMBERS_WRAPPER_CLASS))
        .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(ROW_NUMBER_WRAPPER_STYLE))
        .with_children(AzDomVec::from(vec![top_left_empty_rect, row_numbers]));

        static ACTIVE_SELECTION_HANDLE_STYLE: &[NodeDataInlineCssProperty] = &[
            Normal(CssProperty::const_position(LayoutPosition::Absolute)),
            Normal(CssProperty::const_width(LayoutWidth::const_px(10))),
            Normal(CssProperty::const_height(LayoutHeight::const_px(10))),
            Normal(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(COLOR_407C40_BACKGROUND))),
            Normal(CssProperty::const_bottom(LayoutBottom::const_px(-5))),
            Normal(CssProperty::const_right(LayoutRight::const_px(-5))),
        ];
        static ACTIVE_SELECTION_HANDLE_CLASS: &[IdOrClass] = &[
            IdOrClass::Class(AzString::from_const_str("az-table-active-selection-handle"))
        ];

        // currently active cell handle
        // TODO: add callbacks to modify selection
        let current_active_selection_handle = Dom::div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(ACTIVE_SELECTION_HANDLE_CLASS))
        .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(ACTIVE_SELECTION_HANDLE_STYLE));

        // currently selected cell(s)
        static ACTIVE_SELECTION_CLASS: &[IdOrClass] = &[IdOrClass::Class(AzString::from_const_str("az-table-active-selection"))];

        let current_active_selection = Dom::div()
        .with_inline_css_props(NodeDataInlineCssPropertyVec::from(vec![
            Normal(CssProperty::const_position(LayoutPosition::Absolute)),
            Normal(CssProperty::const_width({
                self.selection.as_ref()
                .map(|selection| LayoutWidth::px(selection.number_of_columns_selected() as f32 * self.default_column_width))
                .unwrap_or(LayoutWidth::zero()) // TODO: replace with transform: scale-x
            })),
            Normal(CssProperty::const_height({
                self.selection.as_ref()
                .map(|selection| LayoutHeight::px(selection.number_of_rows_selected() as f32 * self.default_row_height))
                .unwrap_or(LayoutHeight::zero())  // TODO: replace with transform: scale-y
            })),
            Normal(CssProperty::const_margin_left({
                self.selection.as_ref()
                .map(|selection| LayoutMarginLeft::px(selection.from_top_left.column as f32 * self.default_column_width))
                .unwrap_or(LayoutMarginLeft::zero()) // TODO: replace with transform-y
            })),
            Normal(CssProperty::const_margin_top({
                self.selection.as_ref()
                .map(|selection| LayoutMarginTop::px(selection.from_top_left.row as f32 * self.default_row_height))
                .unwrap_or(LayoutMarginTop::zero()) // TODO: replace with transform-y
            })),
            Normal(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(SELECTED_CELL_BORDER_WIDTH))),
            Normal(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: SELECTED_CELL_BORDER_STYLE })),
            Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: SELECTED_CELL_BORDER_COLOR })),
            Normal(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(SELECTED_CELL_BORDER_WIDTH))),
            Normal(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: SELECTED_CELL_BORDER_STYLE })),
            Normal(CssProperty::const_border_top_color(StyleBorderTopColor { inner: SELECTED_CELL_BORDER_COLOR })),
            Normal(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(SELECTED_CELL_BORDER_WIDTH))),
            Normal(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: SELECTED_CELL_BORDER_STYLE })),
            Normal(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: SELECTED_CELL_BORDER_COLOR })),
            Normal(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(SELECTED_CELL_BORDER_WIDTH))),
            Normal(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: SELECTED_CELL_BORDER_STYLE })),
            Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: SELECTED_CELL_BORDER_COLOR })),
            // don't show the selection when the table doesn't have one
            // TODO: animate / fade in / fade out
            Normal(CssProperty::const_opacity(if self.selection.is_some() { StyleOpacity::const_new(1) } else { StyleOpacity::const_new(0) })),
        ]))
        .with_ids_and_classes(IdOrClassVec::from_const_slice(ACTIVE_SELECTION_CLASS))
        .with_children(AzDomVec::from(vec![current_active_selection_handle]));

        let mut column_doms = columns.map(|col_idx| {

            static COLUMN_NAME_STYLE: &[NodeDataInlineCssProperty] = &[
                Normal(CssProperty::const_height(LayoutHeight::const_px(20))),
                Normal(CssProperty::const_font_family(SANS_SERIF_FONT_FAMILY)),
                Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_2D2D2D })),
                Normal(CssProperty::const_font_size(StyleFontSize::const_px(14))),
                Normal(CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(COLOR_E6E6E6_BACKGROUND))),
                Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
                Normal(CssProperty::const_align_items(LayoutAlignItems::Center)),
                Normal(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
                Normal(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))),
                Normal(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })),
                Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_B5B5B5 })),
                Normal(CssProperty::const_box_shadow_bottom(SHADOW)),
            ];
            static COLUMN_NAME_CLASS: &[IdOrClass] = &[IdOrClass::Class(AzString::from_const_str("az-table-column-name"))];

            static ROWS_IN_COLUMN_STYLE: &[NodeDataInlineCssProperty] = &[
                Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
            ];
            static ROWS_IN_COLUMN_CLASS: &[IdOrClass] = &[IdOrClass::Class(AzString::from_const_str("az-table-rows"))];

            let column_names = Dom::text(column_name_from_number(col_idx).into())
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COLUMN_NAME_CLASS))
            .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(COLUMN_NAME_STYLE));

            // rows in this column, laid out vertically
            let rows_in_this_column = (rows.start..rows.end).map(|row_idx| {

                    let node_type = match self.get_cell_content(&TableCellIndex { row: row_idx, column: col_idx }) {
                        Some(string) => NodeType::Text(string.clone().into()),
                        None => NodeType::Text(DEFAULT_TABLE_CELL_STRING),
                    };

                    static CELL_STYLE: &[NodeDataInlineCssProperty] = &[
                        Normal(CssProperty::const_align_items(LayoutAlignItems::FlexStart)),
                        Normal(CssProperty::const_height(LayoutHeight::const_px(20))),
                        Normal(CssProperty::const_min_height(LayoutMinHeight::const_px(20))),
                        Normal(CssProperty::const_max_height(LayoutMaxHeight::const_px(20))),
                        Normal(CssProperty::const_font_size(StyleFontSize::const_px(14))),
                        Normal(CssProperty::const_text_align(StyleTextAlign::Left)),
                        Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_BLACK })),
                        Normal(CssProperty::const_font_family(SANS_SERIF_FONT_FAMILY)),
                        Normal(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
                        Normal(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
                        Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_D1D1D1 })),
                    ];
                    static CELL_CLASS: &[IdOrClass] = &[IdOrClass::Class(AzString::from_const_str("az-table-cell"))];

                    NodeData::new(node_type)
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(CELL_CLASS))
                    .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(CELL_STYLE))
            })
            .collect::<Dom>()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(ROWS_IN_COLUMN_CLASS))
            .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(ROWS_IN_COLUMN_STYLE));

            static COLUMN_STYLE: &[NodeDataInlineCssProperty] = &[
                Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
                Normal(CssProperty::const_min_width(LayoutMinWidth::const_px(100))),
                Normal(CssProperty::const_max_width(LayoutMaxWidth::const_px(100))),
                Normal(CssProperty::const_width(LayoutWidth::const_px(100))),
                Normal(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))),
                Normal(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })),
                Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_D1D1D1 })),
            ];
            static COLUMN_CLASS: &[IdOrClass] = &[IdOrClass::Class(AzString::from_const_str("az-table-column"))];

            // Column name
            Dom::div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COLUMN_CLASS))
            .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(COLUMN_STYLE))
            .with_children(AzDomVec::from(vec![column_names, rows_in_this_column]))
        })
        .collect::<Vec<Dom>>();

        column_doms.push(current_active_selection);

        let columns_table_container = Dom::div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(COLUMNS_TABLE_CONTAINER_CLASS))
        .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(COLUMNS_TABLE_CONTAINER_STYLE))
        .with_children(column_doms.into());


        static COLUMNS_TABLE_CONTAINER_STYLE: &[NodeDataInlineCssProperty] = &[
            Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
            Normal(CssProperty::const_position(LayoutPosition::Relative)),
        ];
        static COLUMNS_TABLE_CONTAINER_CLASS: &[IdOrClass] = &[IdOrClass::Class(AzString::from_const_str("az-table-container"))];

        static IFRAME_DOM_CONTAINER_STYLE: &[NodeDataInlineCssProperty] = &[
            Normal(CssProperty::const_display(LayoutDisplay::Flex)),
            Normal(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
            Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
        ];
        static IFRAME_DOM_CONTAINER_CLASS: &[IdOrClass] = &[IdOrClass::Class(AzString::from_const_str("az-table-iframe-container"))];

        let dom = Dom::div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(IFRAME_DOM_CONTAINER_CLASS))
        .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(IFRAME_DOM_CONTAINER_STYLE))
        .with_children(AzDomVec::from(vec![row_number_wrapper, columns_table_container]));

        dom
    }
}

impl TableView {

    #[inline]
    pub const fn new(state: TableViewState) -> Self {
        Self { state }
    }

    #[inline]
    pub fn dom(self) -> Dom {

        use azul::css::*;
        use azul::vec::NodeDataInlineCssPropertyVec;
        use azul::dom::NodeDataInlineCssProperty;
        use azul::dom::NodeDataInlineCssProperty::*;

        const IFRAME_STYLE: &[NodeDataInlineCssProperty] = &[
            Normal(CssProperty::const_display(LayoutDisplay::Flex)),
            Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
            Normal(CssProperty::const_width(LayoutWidth::const_percent(100))),
            Normal(CssProperty::const_height(LayoutHeight::const_percent(100))),
            Normal(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
        ];

        Dom::iframe(RefAny::new(self.state), Self::render_table_iframe_contents)
        .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(IFRAME_STYLE))
    }

    extern "C" fn render_table_iframe_contents(state: &mut RefAny, info: IFrameCallbackInfo) -> IFrameCallbackReturn {

        use azul::css::Css;

        let table_view_state = state.downcast_ref::<TableViewState>().unwrap();

        let logical_size = info.bounds.get_logical_size();
        let padding_rows = 0;
        let padding_columns = 0;
        let row_start = 0; // bounds.top / table_view_state.row_height
        let column_start = 0; // bounds.left / table_view_state.column_width

        // workaround for necessary_rows.ceil() not being available on no_std
        let necessary_rows_f32 = logical_size.height as f32 / table_view_state.default_row_height;
        let necessary_rows = if (necessary_rows_f32 * 10.0) as isize % 10_isize != 0 {
            necessary_rows_f32 as usize + 1
        } else {
            necessary_rows_f32 as usize
        };
        let necessary_columns_f32 = logical_size.width as f32 / table_view_state.default_column_width;
        let necessary_columns = if (necessary_columns_f32 * 10.0) as isize % 10_isize != 0 {
            necessary_columns_f32 as usize + 1
        } else {
            necessary_columns_f32 as usize
        };

        let table_height = (necessary_rows + padding_rows) as f32 * table_view_state.default_row_height;
        let table_width = (necessary_columns + padding_columns) as f32 * table_view_state.default_column_width;

        let mut dom = table_view_state.dom(
            row_start..((row_start + necessary_rows + padding_rows)),
            column_start..((column_start + necessary_columns + padding_columns))
        );

        IFrameCallbackReturn {
            dom: dom.style(Css::empty()),
            scroll_size: info.scroll_size,
            scroll_offset: info.scroll_offset,
            virtual_scroll_size: info.virtual_scroll_size,
            virtual_scroll_offset: info.virtual_scroll_offset,
        }
    }
}

impl From<TableView> for Dom  {
    fn from(t: TableView) -> Dom {
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
pub fn column_name_from_number(num: usize) -> String {

    #[inline(always)]
    fn u8_to_char(input: u8) -> u8 {
        'A' as u8 + input
    }

    let mut result = [0;16];
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
    let s = unsafe { ::core::str::from_utf8_unchecked(slice) };
    String::from(s)
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