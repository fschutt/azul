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
    pub fn render(&self, rows: Range<usize>, columns: Range<usize>, style_options: StyleOptions) -> StyledDom {

        use azul::str::String as AzString;

        let sans_serif_font_family = StyleFontFamily { fonts: vec!["sans-serif".into()].into() };

        // Empty rectangle at the top left of the table
        let top_left_empty_rect = Dom::div()
        .with_inline_css(CssProperty::height(LayoutHeight::px(20.0)))
        .with_inline_css(CssProperty::background_content(StyleBackgroundContent::Color(StyleColor::parse("#e6e6e6"))))
        .with_inline_css(CssProperty::border_bottom(StyleBorderBottom::Color(StyleColor::parse("#b5b5b5"))))
        .with_inline_css(CssProperty::border_right(StyleBorderRight::Color(StyleColor::parse("#b5b5b5"))));

        // Row numbers (first column - laid out vertical) - "1", "2", "3"
        let row_numbers = (rows.start..rows.end.saturating_sub(1)).map(|row_idx| {
            NodeData::label(format!("{}", row_idx + 1).into())
            .with_inline_css(CssProperty::font_size(StyleFontSize::px(14.0)))
            .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Row))
            .with_inline_css(CssProperty::justify_content(LayoutJustifyContent::Center))
            .with_inline_css(CssProperty::align_items(LayoutAlignItems::Center))
            .with_inline_css(CssProperty::min_height(LayoutMinHeight::px(20.0)))
            .with_inline_css(CssProperty::border_bottom(StyleBorderBottom::new(PixelValue::px(0.6), StyleBorderStyle::Solid, StyleColor::parse("#b5b5b5"))))
        })
        .collect::<Dom>()
        .with_inline_css(CssProperty::font_family(sans_serif_font_family.clone()))
        .with_inline_css(CssProperty::color(StyleColor::parse("#2d2d2d")))
        .with_inline_css(CssProperty::background_content(StyleBackgroundContent::Color(StyleColor::parse("#e6e6e6"))))
        .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Column))
        .with_inline_css(CssProperty::box_shadow_right(StyleBoxShadowRight::new(PixelValue::px(0.0), PixelValue::px(0.0), PixelValue::px(3.0), StyleColor::black())));

        // first column: contains the "top left rect" + the column
        let row_number_wrapper = Dom::div()
        .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Column))
        .with_inline_css(CssProperty::max_width(LayoutMaxWidth::px(30.0)))
        .with_child(top_left_empty_rect)
        .with_child(row_numbers);

        // currently active cell handle
        let current_active_selection_handle = Dom::div()
        .with_inline_css(CssProperty::position(LayoutPosition::Absolute))
        .with_inline_css(CssProperty::width(LayoutWidth::px(10.0)))
        .with_inline_css(CssProperty::height(LayoutHeight::px(10.0)))
        .with_inline_css(CssProperty::background_content(StyleBackgroundContent::Color(StyleColor::parse("#407c40"))))
        .with_inline_css(CssProperty::bottom(LayoutBottom::px(-5.0)))
        .with_inline_css(CssProperty::right(LayoutRight::px(-5.0)));


        // currently selected cell(s)
        let current_active_selection = Dom::div()
        .with_inline_css(CssProperty::width(LayoutWidth::px(100.0)))
        .with_inline_css(CssProperty::height(LayoutHeight::px(20.0)))
        .with_inline_css(CssProperty::margin_top(LayoutMarginTop::px(500.0))) // TODO: replace with transform-y
        .with_inline_css(CssProperty::margin_left(LayoutMarginLeft::px(100.0))) // TODO: replace with transform-x
        .with_inline_css(CssProperty::position(LayoutPosition::Absolute))
        .with_inline_css(CssProperty::border(StyleBorder::new(PixelValue::px(2.0), StyleBorderStyle::Solid, StyleColor::parse("#407c40"))))
        .with_child(current_active_selection_handle);

        let columns_table_container = columns.map(|col_idx| {

            let column_names = Dom::label(column_name_from_number(col_idx).into())
            .with_inline_css(CssProperty::height(LayoutHeight::px(20.0)))
            .with_inline_css(CssProperty::font_family(StyleFontFamily { fonts: vec!["sans-serif".into()].into()))
            .with_inline_css(CssProperty::color(StyleColor::parse("#2d2d2d")))
            .with_inline_css(CssProperty::font_size(StyleFontSize::px(14.0)))
            .with_inline_css(CssProperty::background_content(StyleBackgroundContent::Color(StyleColor::parse("#e6e6e6"))))
            .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Row))
            .with_inline_css(CssProperty::align_items(LayoutAlignItems::Center))
            .with_inline_css(CssProperty::border_right(StyleBorderRight::new(PixelValue::px(1.0), StyleBorderStyle::Solid, StyleColor::parse("#b5b5b5"))))
            .with_inline_css(CssProperty::box_shadow_bottom(
                StyleBoxShadowBottom::new(PixelValue::px(0.0), PixelValue::px(0.0), PixelValue::px(3.0), StyleColor::black()))
            );


            // rows in this column, laid out vertically
            let rows_in_this_column = (rows.start..rows.end)
                .map(|row_idx| {
                    let node_type = match self.work_sheet.get(&col_idx).and_then(|col| col.get(&row_idx)) {
                        Some(string) => NodeType::Label(string.clone().into()),
                        None => NodeType::Div,
                    };

                    NodeData::new(node_type)
                    .with_inline_css(CssProperty::font_family(sans_serif_font_family.clone()))
                    .with_inline_css(CssProperty::color(StyleColor::black()))
                    .with_inline_css(CssProperty::text_align(LayoutTextAlign::Left))
                    .with_inline_css(CssProperty::align_items(LayoutAlignItems::FlexStart))
                    .with_inline_css(CssProperty::font_size(StyleFontSize::px(14.0)))
                    .with_inline_css(CssProperty::border_bottom(StyleBorderBottom::new(PixelValue::px(1.0), StyleBorderStyle::Solid, StyleColor::parse("#d1d1d1"))))
                    .with_inline_css(CssProperty::height(LayoutHeight::px(20.0)))
                })
                .collect::<Dom>()
                .with_class("__azul-native-table-rows".into());

            // Column name
            Dom::div()
                .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Column))
                .with_inline_css(CssProperty::min_width(LayoutMinWidth::px(100.0)))
                .with_inline_css(CssProperty::border_right(StyleBorderRight::new(PixelValue::px(1.0), StyleBorderStyle::Solid, StyleColor::parse("#d1d1d1"))))
                .with_child(column_names)
                .with_child(rows_in_this_column)
        })
        .collect::<Dom>()
        .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Row))
        .with_inline_css(CssProperty::position(LayoutPosition::Relative))
        .with_child(current_active_selection);


        Dom::div()
        .with_inline_css(CssProperty::display(LayoutDisplay::Flex))
        .with_inline_css(CssProperty::box_sizing(LayoutBoxSizing::BorderBox))
        .with_inline_css(CssProperty::flex_direction(LayoutFlexDirection::Row))
        .with_child(row_number_wrapper)
        .with_child(columns_table_container)
        .style(Css::empty(), style_options)
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
    pub fn dom(self, style_options: StyleOptions) -> StyledDom {
        Dom::iframe(self.state.clone(), Self::render_table_iframe_contents)
            .with_inline_css(CssProperty::display(Display::Flex))
            .with_inline_css(CssProperty::flex_grow(FlexGrow::new(1.0)))
            .with_inline_css(CssProperty::width(Width::percent(100.0)))
            .with_inline_css(CssProperty::height(Height::percent(100.0)))
            .with_inline_css(CssProperty::box_sizing(BoxSizing::BorderBox))
            .with_callback(On::MouseUp.into(), self.state, self.on_mouse_up.cb)
            .style(Css::empty(), style_options)
    }

    pub extern "C" fn default_on_mouse_up(_info: CallbackInfo) -> CallbackReturn {
        println!("table was clicked");
        UpdateScreen::DontRedraw
    }

    extern "C" fn render_table_iframe_contents(info: IFrameCallbackInfo) -> IFrameCallbackReturn {
        println!("rendering table iframe: {:?}", info.get_bounds().get_logical_size());
        fn render_table_iframe_contents_inner(info: IFrameCallbackInfo) -> Option<Dom> {
            let state = info.get_state();
            let table_view_state = state.borrow::<TableViewState>()?;
            let logical_size = info.get_bounds().get_logical_size();
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