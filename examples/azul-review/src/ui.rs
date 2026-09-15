use azul::{
    callbacks::RenderImageCallbackInfo,
    dom::{AccessibilityInfo, IdOrClass, RenderImageCallback},
    image::{ImageRef, RawImage, RawImageData, RawImageFormat},
    menu::{Menu, MenuItem, StringMenuItem},
    prelude::*,
    vec::{IdOrClassVec, MenuItemVec, U8VecRef},
};

use crate::{code, ink, model::Semantic, AppState};

pub const LINE_H: f32 = 15.0;
const PAGE_W: f32 = 1000.0;
const GUTTER_W: f32 = 52.0;

const MARGIN_GUTTER_W: f32 = 260.0;
const PAGE_PAD: f32 = 32.0;
const PAGE_GAP: f32 = 24.0;

pub const METER_PACKET_SAMPLES: usize = 4096;

const METER_SPAN_PACKETS: usize = 240;

fn page_h() -> f32 {
    code::LINES_PER_PAGE as f32 * LINE_H + 40.0
}

fn page_stride() -> f32 {
    PAGE_W + PAGE_GAP
}

pub fn page_of(info: &mut CallbackInfo) -> Option<usize> {
    info.get_dataset(info.get_hit_node())
        .into_option()
        .and_then(|mut d| d.downcast_ref::<PageTag>().map(|t| t.page))
}

pub fn sample(info: &mut CallbackInfo) -> Option<(crate::model::InkPoint, bool)> {
    let p = info.get_cursor_relative_to_node().into_option()?;
    if let Some(pen) = info.get_pen_state().into_option() {
        if pen.in_contact {
            return Some((
                ink::point_from(p.x, p.y, pen.pressure, pen.tilt.x_tilt, pen.tilt.y_tilt),
                pen.is_eraser,
            ));
        }
    }
    Some((ink::point_from(p.x, p.y, 0.0, 0.0, 0.0), false))
}

#[derive(Debug, Clone, Copy)]
pub struct IndexTag {
    pub index: usize,
}

pub fn index_of(info: &mut CallbackInfo) -> Option<usize> {
    info.get_dataset(info.get_hit_node())
        .into_option()
        .and_then(|mut d| d.downcast_ref::<IndexTag>().map(|t| t.index))
}

fn named(name: &str) -> AccessibilityInfo {
    AccessibilityInfo {
        accessibility_name: OptionString::Some(name.into()),
        ..AccessibilityInfo::default()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PageTag {
    pub page: usize,
}

pub extern "C" fn layout(data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let mut d = data.clone();
    let Some(s) = d.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };

    let mut root = Dom::create_body().with_css(
        "display: flex; flex-direction: row; background: #e9e7e2; font-family: sans-serif;",
    );
    root.add_child(sidebar(&s, &data));

    let mut center = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px;");
    center.add_child(toolbar(&s, &data));
    center.add_child(page_rail(&s, &data));
    center.add_child(sheet(&s, &data));
    center.add_child(status_bar(&s));
    root.add_child(center);
    root.with_menu_bar(menu_bar(&data))
}

fn menu_bar(data: &RefAny) -> Menu {
    let mut items: Vec<MenuItem> = Vec::new();

    let mut ink = StringMenuItem::create("Ink");
    for (i, sem) in Semantic::ALL.iter().enumerate() {
        ink = ink.with_child(MenuItem::String(
            StringMenuItem::create(sem.label())
                .with_callback(RefAny::new(IndexTag { index: i }), crate::on_menu_semantic),
        ));
    }
    ink = ink.with_child(MenuItem::Separator);
    ink = ink.with_child(MenuItem::String(
        StringMenuItem::create("Cycle nib").with_callback(data.clone(), crate::on_cycle_tool),
    ));
    ink = ink.with_child(MenuItem::String(
        StringMenuItem::create("Start / stop recording")
            .with_callback(data.clone(), crate::on_toggle_record),
    ));
    items.push(MenuItem::String(ink));

    let mut session = StringMenuItem::create("Session");
    session = session.with_child(MenuItem::String(
        StringMenuItem::create("Save now").with_callback(data.clone(), crate::on_menu_save),
    ));
    session = session.with_child(MenuItem::String(
        StringMenuItem::create("Reveal archive folder")
            .with_callback(data.clone(), crate::on_menu_reveal),
    ));
    items.push(MenuItem::String(session));

    Menu::create(MenuItemVec::copy_from_ptr(&items[0], items.len()))
}

fn sidebar(s: &AppState, data: &RefAny) -> Dom {
    let mut col = Dom::create_div().with_css(
        "display: flex; flex-direction: column; width: 260px; flex-shrink: 0; min-height: 0px; \
         height: 100%; background: #f7f6f3; border-right: 1px solid #cfcbc4;",
    );
    col.add_child(Dom::create_div_with_text("Name").with_css(
        "font-size: 11px; padding: 7px 10px; color: #6b665e; flex-shrink: 0; border-bottom: 1px \
         solid #d8d4cd; background: #efede8;",
    ));

    let mut list = Dom::create_div()
        .with_css("flex-grow: 1; min-height: 0px; overflow-y: auto; overflow-x: hidden;");

    let mut prev: Vec<String> = Vec::new();
    for (i, f) in s.files.iter().enumerate() {
        let parts: Vec<&str> = f.display.split(['/', '\\']).collect();
        let (dirs, name) = parts.split_at(parts.len().saturating_sub(1));
        for (depth, comp) in dirs.iter().enumerate() {
            if prev.get(depth).map(String::as_str) == Some(*comp) {
                continue;
            }
            list.add_child(finder_row(
                comp, depth, "folder", "#4a90d9", false, None, data,
            ));
        }
        prev = dirs.iter().map(|c| (*c).to_string()).collect();
        let leaf = name.first().copied().unwrap_or(f.display.as_str());
        list.add_child(finder_row(
            leaf,
            dirs.len(),
            file_icon(leaf),
            "#7d786f",
            s.current == Some(i),
            Some(i),
            data,
        ));
    }
    col.add_child(list);
    col
}

const fn file_icon(name: &str) -> &'static str {
    let bytes = name.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        if bytes[i] == b'.' {
            let ext = bytes.split_at(i + 1).1;
            return match ext {
                b"rs" | b"c" | b"h" | b"cpp" | b"hpp" | b"py" | b"js" | b"ts" => "code",
                b"sh" => "terminal",
                b"md" => "article",
                b"toml" | b"yml" | b"yaml" | b"json" => "data_object",
                _ => "insert_drive_file",
            };
        }
    }
    "insert_drive_file"
}

fn finder_row(
    label: &str,
    depth: usize,
    icon: &str,
    icon_color: &str,
    selected: bool,
    index: Option<usize>,
    data: &RefAny,
) -> Dom {
    let mut row = Dom::create_div().with_css(
        format!(
            "display: flex; flex-direction: row; align-items: center; gap: 6px; padding: 3px 10px \
             3px {}px; font-size: 11px; flex-shrink: 0; background: {}; color: {};",
            10 + depth * 16,
            if selected { "#3478f6" } else { "transparent" },
            if selected { "#ffffff" } else { "#2b2b2b" },
        )
        .as_str(),
    );
    row.add_child(
        Dom::create_icon(icon).with_css(
            format!(
                "font-size: 14px; color: {};",
                if selected { "#ffffff" } else { icon_color },
            )
            .as_str(),
        ),
    );
    row.add_child(Dom::create_div_with_text(label));
    match index {
        Some(i) => row
            .with_dataset(OptionRefAny::Some(RefAny::new(IndexTag { index: i })))
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                data.clone(),
                crate::on_pick_file,
            ),
        None => row,
    }
}

fn toolbar(s: &AppState, data: &RefAny) -> Dom {
    let mut bar = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 8px; padding: 8px 12px; \
         background: #f7f6f3; border-bottom: 1px solid #cfcbc4;",
    );
    for (i, sem) in Semantic::ALL.iter().enumerate() {
        let c = sem.color();
        let selected = *sem == s.active;
        let mut swatch = Dom::create_div().with_css(
            format!(
                "display: flex; flex-direction: row; align-items: center; gap: 5px; padding: 5px \
                 10px; font-size: 12px; border-radius: 4px; background: rgba({},{},{},{}); color: \
                 {}; border: {};",
                c.r,
                c.g,
                c.b,
                if selected { "1.0" } else { "0.18" },
                if selected { "#ffffff" } else { "#2b2b2b" },
                if selected {
                    "2px solid #2b2b2b"
                } else {
                    "1px solid #cfcbc4"
                },
            )
            .as_str(),
        );
        swatch.add_child(Dom::create_icon(sem.icon()).with_css("font-size: 15px;"));
        swatch.add_child(
            Dom::create_div_with_text(format!("{}", i + 1).as_str())
                .with_css("font-size: 10px; opacity: 0.75;"),
        );
        bar.add_child(
            swatch
                .with_dataset(OptionRefAny::Some(RefAny::new(IndexTag { index: i })))
                .with_accessibility_info(named(sem.label()))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    crate::on_pick_semantic,
                ),
        );
    }
    let mut nib = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 6px; margin-left: 16px; \
         padding: 5px 12px; font-size: 12px; border-radius: 4px; background: #ffffff; color: \
         #2b2b2b; border: 1px dashed #a9a49b;",
    );
    nib.add_child(Dom::create_icon(s.tool.icon()).with_css("font-size: 15px;"));
    nib.add_child(Dom::create_div_with_text(s.tool.label()));
    bar.add_child(nib);

    let rec = s.recording.is_some();
    if rec {
        bar.add_child(meter(s));
    }
    let mut record = Dom::create_div().with_css(if rec {
        "display: flex; flex-direction: row; align-items: center; gap: 6px; margin-left: 10px; \
         padding: 5px 12px; font-size: 12px; border-radius: 4px; background: #d62d20; color: white;"
    } else {
        "display: flex; flex-direction: row; align-items: center; gap: 6px; margin-left: auto; \
         padding: 5px 12px; font-size: 12px; border-radius: 4px; background: #ffffff; color: \
         #2b2b2b; border: 1px solid #cfcbc4;"
    });
    record.add_child(
        Dom::create_icon(if rec { "fiber_manual_record" } else { "mic" })
            .with_css("font-size: 15px;"),
    );
    record.add_child(Dom::create_div_with_text(if rec {
        "recording"
    } else {
        "record"
    }));
    bar.add_child(
        record
            .with_accessibility_info(named(if rec {
                "stop recording"
            } else {
                "start recording"
            }))
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                data.clone(),
                crate::on_toggle_record,
            ),
    );
    bar
}

fn meter(s: &AppState) -> Dom {
    let packets = s.level_samples / METER_PACKET_SAMPLES;
    let filled = (packets % METER_SPAN_PACKETS) as f32 / METER_SPAN_PACKETS as f32;
    let mut wrap = Dom::create_div().with_css(
        "margin-left: auto; display: flex; flex-direction: row; align-items: center; gap: 6px;",
    );
    wrap.add_child(
        Dom::create_div_with_text(format!("{packets} pkt").as_str())
            .with_css("font-size: 11px; color: #6b665e; font-family: monospace;"),
    );
    let mut holder = Dom::create_div().with_css("width: 160px;");
    holder.add_child(ProgressBar::create(filled * 100.0).dom());
    wrap.add_child(holder);
    wrap
}

pub const STRIP_ID: &str = "sheet-strip";

fn page_rail(s: &AppState, data: &RefAny) -> Dom {
    let mut rail = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 3px; padding: 4px 12px; \
         background: #efede8; border-bottom: 1px solid #cfcbc4; overflow-x: auto; overflow-y: \
         hidden; flex-shrink: 0; width: 100%; box-sizing: border-box;",
    );
    let Some(file) = s.file() else { return rail };
    for page in 0..file.page_count() {
        let here = page == s.visible_page;
        let css = if here {
            "padding: 2px 9px; font-size: 11px; font-family: monospace; border-radius: 3px; \
             background: #2b2b2b; color: #ffffff; flex-shrink: 0;"
        } else {
            "padding: 2px 9px; font-size: 11px; font-family: monospace; border-radius: 3px; \
             background: #ffffff; color: #55514a; border: 1px solid #d8d4cd; flex-shrink: 0;"
        };
        rail.add_child(
            Dom::create_div_with_text(format!("{}", page + 1).as_str())
                .with_dataset(OptionRefAny::Some(RefAny::new(IndexTag { index: page })))
                .with_css(css)
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    crate::on_jump_to_page,
                ),
        );
    }
    rail
}

pub fn scroll_to_page(info: &mut CallbackInfo, page: usize) {
    let dom = info.get_hit_node().dom;
    let node = info.get_node_id_by_id_attribute(dom, STRIP_ID);
    info.scroll_to(
        dom,
        node,
        LogicalPosition::create(page as f32 * page_stride(), 0.0),
    );
}

fn sheet(s: &AppState, data: &RefAny) -> Dom {
    let mut area = Dom::create_div().with_css(
        "flex-grow: 1; min-height: 0px; background: #e9e7e2; display: flex; flex-direction: \
         column; overflow: hidden; padding: 18px;",
    );
    if s.file().is_none() {
        area.add_child(
            Dom::create_div_with_text("Open a file to begin")
                .with_css("color: #7a756c; padding: 40px;"),
        );
        return area;
    }
    area.add_child(
        Dom::create_virtual_view(
            RefAny::new(SheetStrip { app: data.clone() }),
            sheets_virtual_view,
        )
        .with_ids_and_classes(IdOrClassVec::from_item(IdOrClass::id(STRIP_ID)))
        .with_css("flex-grow: 1; min-height: 0px; width: 100%;"),
    );
    area
}

struct SheetStrip {
    app: RefAny,
}

extern "C" fn sheets_virtual_view(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let mut app = {
        let Some(strip) = data.downcast_ref::<SheetStrip>() else {
            return VirtualViewReturn::default();
        };
        strip.app.clone()
    };
    let cb_app = app.clone();

    let stride = page_stride();
    let strip_h = page_h();

    let (dom, first, count, total, leftmost) = {
        let Some(s) = app.downcast_ref::<AppState>() else {
            return VirtualViewReturn::default();
        };
        let Some(file) = s.file() else {
            return VirtualViewReturn::default();
        };
        let total = file.page_count().max(1);

        let viewport_w = info.bounds.get_logical_size().width;
        let leftmost = ((info.scroll_offset.x.max(0.0) / stride).round() as usize).min(total - 1);
        let first_visible = (info.scroll_offset.x.max(0.0) / stride) as usize;
        let first = first_visible.saturating_sub(1);
        let visible = (viewport_w / stride).ceil() as usize + 2;
        let count = visible.max(2).min(total.saturating_sub(first));

        let mut row = Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: flex-start;");
        for page in first..first + count {
            row.add_child(page_sheet(&s, &cb_app, file, page));
        }
        (row, first, count, total, leftmost)
    };

    if let Some(mut s) = app.downcast_mut::<AppState>() {
        s.visible_page = leftmost;
    }

    VirtualViewReturn::with_dom(
        dom,
        LogicalRect::create(
            LogicalPosition::create(first as f32 * stride, 0.0),
            LogicalSize::create(count as f32 * stride, strip_h),
        ),
        LogicalRect::create(
            LogicalPosition::create(0.0, 0.0),
            LogicalSize::create(total as f32 * stride, strip_h),
        ),
    )
}

fn page_sheet(s: &AppState, data: &RefAny, file: &code::SourceFile, page: usize) -> Dom {
    let (first_line, lines) = file.page(page);
    let mut sheet = Dom::create_div().with_css(
        format!(
            "position: relative; width: {}px; height: {}px; background: #ffffff; border: 1px \
             solid #b9b4ab; box-shadow: 0px 1px 4px #00000030; margin-right: {}px; flex-shrink: \
             0; box-sizing: border-box; padding: {}px; overflow: hidden;",
            PAGE_W as isize,
            page_h() as isize,
            PAGE_GAP as isize,
            PAGE_PAD as isize,
        )
        .as_str(),
    );

    sheet.add_child(
        Dom::create_div().with_css(
            format!(
                "position: absolute; top: {}px; bottom: {}px; left: {}px; width: 1px; background: \
                 #ece8e1;",
                PAGE_PAD as isize / 2,
                PAGE_PAD as isize / 2,
                (PAGE_W - MARGIN_GUTTER_W) as isize,
            )
            .as_str(),
        ),
    );

    let mut col = Dom::create_div().with_css(
        format!(
            "display: flex; flex-direction: column; width: {}px; overflow: hidden;",
            (PAGE_W - MARGIN_GUTTER_W - PAGE_PAD) as isize,
        )
        .as_str(),
    );
    for (i, line) in lines.iter().enumerate() {
        let mut row = Dom::create_div()
            .with_css(format!("display: flex; flex-direction: row; height: {LINE_H}px;").as_str());
        row.add_child(
            Dom::create_div_with_text(format!("{}", first_line + i).as_str()).with_css(
                format!(
                    "width: {}px; flex-shrink: 0; text-align: right; padding-right: 10px; \
                     font-family: monospace; font-size: 11px; color: #b0aaa0;",
                    GUTTER_W as isize - 10,
                )
                .as_str(),
            ),
        );
        row.add_child(Dom::create_div_with_text(line.as_str()).with_css(
            "font-family: monospace; font-size: 11px; color: #1f1f1f; white-space: pre;",
        ));
        col.add_child(row);
    }
    sheet.add_child(col);

    let page_strokes: Vec<_> = s.strokes.iter().filter(|st| st.page == page).collect();
    let has_live = s.live.as_ref().is_some_and(|l| l.page == page);
    let cache = RefAny::new(InkLayer {
        strokes: page_strokes.into_iter().cloned().collect(),
        live: if has_live { s.live.clone() } else { None },
    });
    sheet.add_child(
        Dom::create_image(ImageRef::callback(
            RenderImageCallback::create(render_ink).to_core(),
            cache,
        ))
        .with_dataset(OptionRefAny::Some(RefAny::new(PageTag { page })))
        .with_css(
            format!(
                "position: absolute; top: 0px; left: 0px; width: {}px; height: {}px;",
                PAGE_W as isize,
                page_h() as isize,
            )
            .as_str(),
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            data.clone(),
            crate::on_ink_down,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseOver),
            data.clone(),
            crate::on_ink_move,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            data.clone(),
            crate::on_ink_up,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::RightMouseUp),
            data.clone(),
            crate::on_cycle_tool_back,
        ),
    );
    sheet
}

struct InkLayer {
    strokes: Vec<crate::model::Stroke>,
    live: Option<crate::model::Stroke>,
}

extern "C" fn render_ink(mut data: RefAny, info: RenderImageCallbackInfo) -> ImageRef {
    let size = info.get_bounds().get_logical_size();
    let (w, h) = (size.width.max(1.0) as u32, size.height.max(1.0) as u32);
    let Some(layer) = data.downcast_ref::<InkLayer>() else {
        return ImageRef::null_image(
            w as usize,
            h as usize,
            RawImageFormat::RGBA8,
            U8VecRef::from(&[][..]),
        );
    };
    let mut all: Vec<&crate::model::Stroke> = layer.strokes.iter().collect();
    if let Some(l) = layer.live.as_ref() {
        all.push(l);
    }
    let buf = ink::rasterize_page(&all, w, h);
    let img = RawImage {
        pixels: RawImageData::U8(buf.into()),
        width: w as usize,
        height: h as usize,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    };
    ImageRef::new_rawimage(img)
        .into_option()
        .unwrap_or_else(|| {
            ImageRef::null_image(
                w as usize,
                h as usize,
                RawImageFormat::RGBA8,
                U8VecRef::from(&[][..]),
            )
        })
}

fn status_bar(s: &AppState) -> Dom {
    let pages = s.file().map_or(0, code::SourceFile::page_count);
    let clips = s.clips.len() + usize::from(s.recording.is_some());
    let mut bar = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 16px; padding: 5px 12px; \
         font-size: 11px; color: #55514a; background: #f7f6f3; border-top: 1px solid #cfcbc4;",
    );
    bar.add_child(Dom::create_div_with_text(
        format!(
            "{pages} sheets  ·  {} strokes  ·  {clips} clips",
            s.strokes.len()
        )
        .as_str(),
    ));
    bar.add_child(Dom::create_div_with_text(s.status.as_str()).with_css("margin-left: auto;"));
    bar
}
