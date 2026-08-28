//! #22 repro: mouse-scroll a VERY LARGE div — BOTH code paths side by side.
//!
//! LEFT (red border)   = plain `overflow-y: auto` div containing one huge
//!                       content column (the non-VirtualView scroll-frame
//!                       path). 500 rows × 100px = 50,000px of content.
//! RIGHT (green border) = a VirtualView that MATERIALIZES only the visible
//!                       rows but declares the same 50,000px
//!                       `virtual_scroll_size` (the VirtualView path).
//!
//! Both sides must: paint a proportional scrollbar, scroll with the mouse
//! wheel, and keep CONTENT moving with the thumb. Live-run bug (2026-08-12):
//! the wheel moved the SCROLLBAR but not the CONTENT — this window isolates
//! that for both paths at once. The VV node carries `overflow-y: auto` like
//! the working infinity example; if the plain side scrolls and the VV side
//! does not, the delta is in the VirtualView wheel routing itself.

use azul::option::OptionDom;
use azul::prelude::*;

const ROWS: usize = 500;
const ROW_H: f32 = 100.0;

struct DataModel;

fn row(i: usize) -> Dom {
    let color = if i % 2 == 0 { "#e3f2fd" } else { "#bbdefb" };
    let mut item = Dom::create_div();
    item.set_css(
        format!(
            "height: {ROW_H}px; background-color: {color}; \
             border-bottom: 1px solid #90a4ae; padding-left: 8px;"
        )
        .as_str(),
    );
    item.add_child(Dom::create_p_with_text(format!("row {i}").as_str()));
    item
}

extern "C" fn vv_rows(mut _data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let first = (info.scroll_offset.y.max(0.0) / ROW_H) as usize;
    let visible = (info.bounds.get_logical_size().height / ROW_H).ceil() as usize + 2;
    let end = (first + visible).min(ROWS);

    let mut col = Dom::create_div();
    col.set_css("display: block;");
    for i in first..end {
        col.add_child(row(i));
    }

    // Each size+offset pair is one rect now: `materialized` is the slice
    // actually rendered and where it sits, `virtual_rect` is the whole
    // document the scrollbar represents.
    VirtualViewReturn {
        dom: OptionDom::Some(col),
        materialized: LogicalRect::create(
            LogicalPosition::create(0.0, first as f32 * ROW_H),
            LogicalSize::create(0.0, (end - first) as f32 * ROW_H),
        ),
        virtual_rect: LogicalRect::create(
            LogicalPosition::create(0.0, 0.0),
            LogicalSize::create(0.0, ROWS as f32 * ROW_H),
        ),
    }
}

extern "C" fn layout(mut data: RefAny, _: LayoutCallbackInfo) -> Dom {
    // LEFT: the plain overflow path — one enormous real DOM.
    let mut content = Dom::create_div();
    content.set_css("display: block;");
    for i in 0..ROWS {
        content.add_child(row(i));
    }
    let mut plain = Dom::create_div();
    plain.set_css(
        "flex-grow: 1; height: 100%; overflow-y: auto; \
         border: 2px solid #f44336; margin: 8px;",
    );
    plain.add_child(content);

    // RIGHT: the VirtualView path — only ~visible rows exist at a time.
    let vview = Dom::create_virtual_view(data.clone(), vv_rows)
        .with_css("flex-grow: 1; min-height: 0px; width: 100%; overflow-y: auto;");
    let mut vv_wrap = Dom::create_div();
    vv_wrap.set_css(
        "flex-grow: 1; height: 100%; border: 2px solid #4caf50; margin: 8px; \
         display: flex; flex-direction: column;",
    );
    vv_wrap.add_child(vview);

    let mut body = Dom::create_body();
    body.set_css("display: flex; flex-direction: row; height: 100%; margin: 0;");
    body.add_child(plain);
    body.add_child(vv_wrap);
    body
}

fn main() {
    let app = App::create(RefAny::new(DataModel), AppConfig::create());
    app.run(WindowCreateOptions::create(layout));
}
