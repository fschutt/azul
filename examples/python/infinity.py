from azul import *

TOTAL_ROWS = 1000000
ROW_HEIGHT = 22.0
VISIBLE_ROWS = 60

COL_LABELS = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L"]
COL_TITLES = ["Order", "Product", "Region", "Qty", "Unit", "Net",
              "Tax", "Total", "Q1", "Q2", "Q3", "Q4"]
PRODUCTS = ["Widget", "Gasket", "Flange", "Bearing",
            "Bracket", "Spindle", "Coupler", "Sleeve"]
REGIONS = ["North", "South", "East", "West", "Central"]

COL_WIDTH = 92
ROW_HEAD_WIDTH = 52

SCROLL = EventFilter.Hover(HoverEventFilter.Scroll)
CLICK = EventFilter.Hover(HoverEventFilter.MouseUp)
PAGE = 25


class SheetState:
    def __init__(self):
        self.total_rows = TOTAL_ROWS
        self.first_row = 0


def hash2(row, col):
    h = (row * 2654435761 ^ col * 40503) & 0xFFFFFFFF
    h ^= h >> 13
    h = (h * 1274126177) & 0xFFFFFFFF
    return h ^ (h >> 16)


def cell_text(row, col):
    h = hash2(row, col)
    if col == 0:
        return "SO-%06d" % (100000 + row)
    if col == 1:
        return PRODUCTS[h % len(PRODUCTS)]
    if col == 2:
        return REGIONS[h % len(REGIONS)]
    if col == 3:
        return str(h % 90 + 10)
    return "%d.%02d" % (h % 900 + 10, h % 100)


def cell(text, css):
    return (Dom.create_div()
            .with_css(css)
            .with_child(Dom.create_p_with_text(text)))


def column_header():
    header = (Dom.create_div()
              .with_css("display:flex;flex-direction:row;background:#dfe3ea;"
                        "border-bottom:1px solid #9aa2ae;"))
    header = header.with_child(cell("", (
        "width:%dpx;min-width:%dpx;height:24px;line-height:24px;"
        "border-right:1px solid #9aa2ae;background:#d3d8e0;" % (ROW_HEAD_WIDTH, ROW_HEAD_WIDTH))))
    for label, title in zip(COL_LABELS, COL_TITLES):
        header = header.with_child(cell("%s   %s" % (label, title), (
            "width:%dpx;min-width:%dpx;height:24px;line-height:24px;padding-left:6px;"
            "font-size:11px;font-weight:bold;color:#33404f;overflow:hidden;"
            "border-right:1px solid #9aa2ae;" % (COL_WIDTH, COL_WIDTH))))
    return header


def sheet_rows(data):
    grid = Dom.create_div()
    end = min(data.first_row + VISIBLE_ROWS, data.total_rows)
    for row_idx in range(data.first_row, end):
        band = "#ffffff" if row_idx % 2 == 0 else "#f6f8fb"
        row = Dom.create_div().with_css(
            "display:flex;flex-direction:row;height:%dpx;background:%s;" % (ROW_HEIGHT, band))
        row = row.with_child(cell(str(row_idx + 1), (
            "width:%dpx;min-width:%dpx;height:%dpx;line-height:%dpx;text-align:center;"
            "font-size:11px;color:#444444;background:#eceff4;"
            "border-right:1px solid #b6bcc6;border-bottom:1px solid #d7dbe2;"
            % (ROW_HEAD_WIDTH, ROW_HEAD_WIDTH, ROW_HEIGHT, ROW_HEIGHT))))
        for col in range(len(COL_LABELS)):
            align = "right" if col >= 3 else "left"
            row = row.with_child(cell(cell_text(row_idx, col), (
                "width:%dpx;min-width:%dpx;height:%dpx;line-height:%dpx;padding-left:6px;"
                "padding-right:6px;font-size:12px;color:#1f2933;text-align:%s;overflow:hidden;"
                "border-right:1px solid #d7dbe2;border-bottom:1px solid #d7dbe2;"
                % (COL_WIDTH, COL_WIDTH, ROW_HEIGHT, ROW_HEIGHT, align))))
        grid = grid.with_child(row)
    return grid


def scroll_by(data, rows):
    new_first = max(0, min(data.total_rows - VISIBLE_ROWS, data.first_row + rows))
    if new_first == data.first_row:
        return Update.DoNothing
    data.first_row = new_first
    return Update.RefreshDom


def on_scroll(data, info):
    return scroll_by(data, PAGE)


def on_page_down(data, info):
    return scroll_by(data, PAGE)


def on_page_up(data, info):
    return scroll_by(data, -PAGE)


def pager_button(text, data, callback):
    return (Dom.create_div()
            .with_css("padding:2px 10px;margin-right:6px;background:#ffffff;color:#33404f;"
                      "border:1px solid #b6bcc6;font-size:11px;cursor:pointer;")
            .with_child(Dom.create_p_with_text(text))
            .with_callback(CLICK, data, callback))


def layout(data, info):
    title = (Dom.create_div()
             .with_css("padding:8px 12px;background:#217346;color:white;"
                       "font-size:13px;font-weight:bold;")
             .with_child(Dom.create_p_with_text(
                 "Sheet1  -  %d rows x %d columns" % (data.total_rows, len(COL_LABELS)))))

    viewport = (Dom.create_div()
                .with_css("flex-grow:1;overflow-y:auto;overflow-x:hidden;background:#ffffff;")
                .with_callback(SCROLL, data, on_scroll)
                .with_child(sheet_rows(data)))

    status = (Dom.create_div()
              .with_css("display:flex;flex-direction:row;align-items:center;padding:4px 12px;"
                        "background:#f1f3f6;border-top:1px solid #c9ced6;color:#55606e;"
                        "font-size:11px;")
              .with_child(pager_button("Prev", data, on_page_up))
              .with_child(pager_button("Next", data, on_page_down))
              .with_child(Dom.create_p_with_text(
                  "rows %d - %d of %d" % (data.first_row + 1,
                                          data.first_row + VISIBLE_ROWS,
                                          data.total_rows))))

    return (Dom.create_body()
            .with_css("display:flex;flex-direction:column;height:100%;margin:0;padding:0;"
                      "font-family:sans-serif;background:#ffffff;")
            .with_child(title)
            .with_child(column_header())
            .with_child(viewport)
            .with_child(status))


state = SheetState()
window = WindowCreateOptions.create(layout)
app = App.create(state, AppConfig.create())
app.run(window)
