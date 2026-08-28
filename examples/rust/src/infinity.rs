use azul::option::OptionDom;
use azul::prelude::*;

const TOTAL_ROWS: usize = 1_000_000;
const ROW_HEIGHT: f32 = 22.0;
const VISIBLE_ROWS: usize = 48;
const COL_WIDTH: f32 = 92.0;
const ROW_HEAD_WIDTH: f32 = 52.0;

const COL_LABELS: &[&str] = &["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L"];
const COL_TITLES: &[&str] = &[
    "Order", "Product", "Region", "Qty", "Unit", "Net", "Tax", "Total", "Q1", "Q2", "Q3", "Q4",
];
const PRODUCTS: &[&str] = &[
    "Widget", "Gasket", "Flange", "Bearing", "Bracket", "Spindle", "Coupler", "Sleeve",
];
const REGIONS: &[&str] = &["North", "South", "East", "West", "Central"];

struct SheetState {
    total_rows: usize,
}

fn hash2(row: usize, col: usize) -> u32 {
    let mut h = (row as u32).wrapping_mul(2_654_435_761) ^ (col as u32).wrapping_mul(40_503);
    h ^= h >> 13;
    h = h.wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}

fn cell_text(row: usize, col: usize) -> std::string::String {
    let h = hash2(row, col);
    match col {
        0 => format!("SO-{:06}", 100_000 + row),
        1 => PRODUCTS[h as usize % PRODUCTS.len()].to_string(),
        2 => REGIONS[h as usize % REGIONS.len()].to_string(),
        3 => format!("{}", h % 90 + 10),
        _ => format!("{}.{:02}", h % 900 + 10, h % 100),
    }
}

fn cell(text: &str, css: &str) -> Dom {
    Dom::create_div_with_text(text).with_css(css)
}

extern "C" fn render_rows(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let total = match data.downcast_ref::<SheetState>() {
        Some(s) => s.total_rows,
        None => return VirtualViewReturn::default(),
    };

    let scroll_y = info.scroll_offset.y.max(0.0);
    let first_row = ((scroll_y / ROW_HEIGHT) as usize).min(total.saturating_sub(1));
    let count = VISIBLE_ROWS.min(total - first_row);

    let mut container = Dom::create_div();

    for i in 0..count {
        let row_idx = first_row + i;
        let band = if row_idx % 2 == 0 {
            "#ffffff"
        } else {
            "#f6f8fb"
        };

        let mut row = Dom::create_div().with_css(format!(
            "display: flex; flex-direction: row; height: {ROW_HEIGHT}px; background: {band};"
        ));

        row.add_child(cell(
            &format!("{}", row_idx + 1),
            &format!(
                "width: {ROW_HEAD_WIDTH}px; min-width: {ROW_HEAD_WIDTH}px; height: {ROW_HEIGHT}px; \
                 line-height: {ROW_HEIGHT}px; text-align: center; font-size: 11px; color: #444444; \
                 background: #eceff4; border-right: 1px solid #b6bcc6; \
                 border-bottom: 1px solid #d7dbe2;"
            ),
        ));

        for c in 0..COL_LABELS.len() {
            let align = if c >= 3 { "right" } else { "left" };
            row.add_child(cell(
                &cell_text(row_idx, c),
                &format!(
                    "width: {COL_WIDTH}px; min-width: {COL_WIDTH}px; height: {ROW_HEIGHT}px; \
                     line-height: {ROW_HEIGHT}px; padding-left: 6px; padding-right: 6px; \
                     font-size: 12px; color: #1f2933; text-align: {align}; overflow: hidden; \
                     border-right: 1px solid #d7dbe2; border-bottom: 1px solid #d7dbe2;"
                ),
            ));
        }

        container.add_child(row);
    }

    let sheet_width = ROW_HEAD_WIDTH + COL_LABELS.len() as f32 * COL_WIDTH;

    VirtualViewReturn {
        dom: OptionDom::Some(container),
        materialized: LogicalRect::create(
            LogicalPosition::create(0.0, first_row as f32 * ROW_HEIGHT),
            LogicalSize::create(sheet_width, count as f32 * ROW_HEIGHT),
        ),
        virtual_rect: LogicalRect::create(
            LogicalPosition::create(0.0, 0.0),
            LogicalSize::create(sheet_width, total as f32 * ROW_HEIGHT),
        ),
    }
}

fn column_header() -> Dom {
    let mut header = Dom::create_div().with_css(
        "display: flex; flex-direction: row; background: #dfe3ea; \
         border-bottom: 1px solid #9aa2ae;",
    );

    header.add_child(cell(
        "",
        &format!(
            "width: {ROW_HEAD_WIDTH}px; min-width: {ROW_HEAD_WIDTH}px; height: 24px; \
             line-height: 24px; border-right: 1px solid #9aa2ae; background: #d3d8e0;"
        ),
    ));

    for (label, title) in COL_LABELS.iter().zip(COL_TITLES) {
        header.add_child(cell(
            &format!("{label}   {title}"),
            &format!(
                "width: {COL_WIDTH}px; min-width: {COL_WIDTH}px; height: 24px; line-height: 24px; \
                 padding-left: 6px; font-size: 11px; font-weight: bold; color: #33404f; \
                 overflow: hidden; border-right: 1px solid #9aa2ae;"
            ),
        ));
    }

    header
}

extern "C" fn layout(data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let title = Dom::create_div_with_text(format!(
        "Sheet1  -  {} rows x {} columns",
        TOTAL_ROWS,
        COL_LABELS.len()
    ))
    .with_css(
        "padding: 8px 12px; background: #217346; color: white; font-size: 13px; \
         font-weight: bold;",
    );

    let vview = Dom::create_virtual_view(data.clone(), render_rows).with_css(
        "display: flex; flex-grow: 1; overflow-y: auto; overflow-x: hidden; background: #ffffff;",
    );

    let status =
        Dom::create_div_with_text("Ready   -   only the visible band of cells exists in the DOM")
            .with_css(
                "padding: 4px 12px; background: #f1f3f6; border-top: 1px solid #c9ced6; \
         color: #55606e; font-size: 11px;",
            );

    Dom::create_body()
        .with_css(
            "display: flex; flex-direction: column; height: 100%; margin: 0; padding: 0; \
             font-family: sans-serif; background: #ffffff;",
        )
        .with_child(title)
        .with_child(column_header())
        .with_child(vview)
        .with_child(status)
}

fn main() {
    let data = RefAny::new(SheetState {
        total_rows: TOTAL_ROWS,
    });
    let app = App::create(data, AppConfig::create());
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = "Infinity - 1M row spreadsheet".into();
    window.window_state.size.dimensions.width = ROW_HEAD_WIDTH + 12.0 * COL_WIDTH + 18.0;
    window.window_state.size.dimensions.height = 620.0;
    app.run(window);
}
