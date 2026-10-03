//! AzPdf's window: the start screen (the empty state and the recent
//! documents) or the document shell - the navigation pane (Pages / Outline),
//! the toolbar over the page view, the search hits in the side pane, the
//! status bar. The page view and the thumbnail rail are VirtualViews: only
//! the pages in view (and one either side) exist in the DOM, each an image
//! of the page as azul drew it, or a blank page with its number while it is
//! being drawn. The pages are paper: white in both modes.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DropDownOnChoiceChangeCallbackType,
        SegmentedOnChangeCallbackType, TextInputOnVirtualKeyDownCallbackType,
    },
    image::ImageRef,
    prelude::*,
    shells::{DocumentShell, ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    vec::StringVec,
    widgets::{DropDown, Segmented, StatusBar, StatusBarSegment},
};
use azul_appkit::ui as kit;

use crate::{
    ids,
    model::{file_title, size_label, Strip, Zoom, PAGE_GAP, VIEW_PAD},
    AppState, Nav,
};

/// A column that takes the rest of its parent.
const COLUMN: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                      min-width: 0px;";

/// A row of toolbar items.
const BAR: &str = "display: flex; flex-direction: row; align-items: center; gap: 6px; padding: \
                   6px 10px; border-bottom: 1px solid system:separator; flex-shrink: 0;";

/// The most search hits the side pane lists.
const MAX_HIT_ROWS: usize = 500;

/// A list item's index (a page, a recent document) in its dataset.
#[derive(Debug, Clone, Copy)]
pub struct IndexTag {
    pub index: usize,
}

/// The [`IndexTag`] of the node the event hit.
pub fn index_of(info: &mut CallbackInfo) -> Option<usize> {
    info.get_dataset(info.get_hit_node())
        .into_option()
        .and_then(|mut d| d.downcast_ref::<IndexTag>().map(|t| t.index))
}

fn tag(index: usize) -> OptionRefAny {
    OptionRefAny::Some(RefAny::new(IndexTag { index }))
}

fn strs(items: &[&str]) -> StringVec {
    StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect())
}

pub extern "C" fn layout(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let mut d = data.clone();
    let Some(s) = d.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };
    let content = if kit::settings_open(&s.kit) {
        // azul-appkit's settings page: Appearance, Data, Shortcuts, About.
        Dom::create_div()
            .with_css(COLUMN)
            .with_child(kit::title_row(crate::SPEC.name))
            .with_child(kit::settings_page(&s.kit, Vec::new()))
    } else if s.doc.is_none() {
        DocumentShell::create(start_screen(&s, &data))
            .office_shell()
            .with_title_row(kit::title_row(crate::SPEC.name))
            .with_status_bar(status_bar(&s))
            .dom()
    } else {
        let title = s
            .doc
            .as_ref()
            .map_or_else(String::new, |d| format!("{} \u{2014} AzPdf", d.title));
        let document = Dom::create_div()
            .with_css(COLUMN)
            .with_child(toolbar(&s, &data))
            .with_child(page_view(&data));
        let mut shell = DocumentShell::create(document)
            .with_navigation(navigation(&s, &data))
            .with_navigation_ratio(0.18);
        if s.search.ran || s.search.running {
            shell = shell.with_side_pane(hits_pane(&s, &data));
        }
        shell
            .office_shell()
            .with_title_row(kit::title_row(title.as_str()))
            .with_status_bar(status_bar(&s))
            .dom()
    };
    ShellThemeScope::create(Dom::create_div().with_css(COLUMN).with_child(content))
        .with_accent(ShellThemeAccent::Clay)
        .body()
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            data.clone(),
            crate::on_key,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::DroppedFile),
            data.clone(),
            crate::on_dropped,
        )
}

// ==== The start screen ====

fn start_screen(s: &AppState, data: &RefAny) -> Dom {
    let mut column = Dom::create_div().with_id(ids::START).with_css(
        "display: flex; flex-direction: column; align-items: center; flex-grow: 1; min-height: \
         0px; overflow-y: auto; padding: 32px;",
    );
    let (title, detail) = if let Some(path) = s.loading.as_ref() {
        ("Opening\u{2026}".to_string(), file_title(path))
    } else if let Some(why) = s.error.as_ref() {
        ("The PDF could not be opened".to_string(), why.clone())
    } else {
        (
            "No document open".to_string(),
            "Open a PDF, drop one on this window, or pick a recent document.".to_string(),
        )
    };
    column.add_child(
        ShellEmptyState::create(title.as_str())
            .with_icon("picture_as_pdf")
            .with_detail(detail.as_str())
            .with_action_label("Open PDF\u{2026}")
            .with_on_action(data.clone(), crate::on_open as ButtonOnClickCallbackType)
            .dom(),
    );
    if s.recent.docs.is_empty() {
        return column;
    }
    let mut list = Dom::create_div().with_id(ids::RECENT).with_css(
        "display: flex; flex-direction: column; width: 560px; margin-top: 24px; border: 1px \
         solid system:separator; border-radius: 6px;",
    );
    list.add_child(Dom::create_h2_with_text("Recent").with_css(
        "font-size: 13px; font-weight: 600; margin: 0px; padding: 8px 12px; border-bottom: 1px \
         solid system:separator;",
    ));
    for (i, doc) in s.recent.docs.iter().enumerate() {
        let mut row = Dom::create_div()
            .with_id(ids::numbered(ids::RECENT_PREFIX, i))
            .with_css(
                "display: flex; flex-direction: row; align-items: center; gap: 10px; padding: \
                 8px 12px; border-bottom: 1px solid system:separator;",
            )
            .with_dataset(tag(i))
            .with_accessibility_name(doc.title.as_str())
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                data.clone(),
                crate::on_recent,
            );
        row.add_child(
            Dom::create_icon("picture_as_pdf").with_css("font-size: 18px; color: system:accent;"),
        );
        let mut text = Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px;");
        text.add_child(Dom::create_span_with_text(doc.title.as_str()).with_css("font-size: 13px;"));
        text.add_child(Dom::create_span_with_text(doc.path.as_str()).with_css(
            "font-size: 11px; color: system:secondary-text; white-space: nowrap; overflow: hidden;",
        ));
        row.add_child(text);
        row.add_child(
            Dom::create_span_with_text(
                format!("p. {} of {}", doc.last_page + 1, doc.pages.max(1)).as_str(),
            )
            .with_css("font-size: 11px; color: system:secondary-text; flex-shrink: 0;"),
        );
        list.add_child(row);
    }
    column.add_child(list);
    column
}

// ==== The toolbar ====

/// A button with only an icon (the caller names it for screen readers).
fn icon_button(icon: &str, data: &RefAny, cb: ButtonOnClickCallbackType) -> Button {
    Button::create("")
        .with_icon(icon)
        .with_on_click(data.clone(), cb)
}

fn separator() -> Dom {
    Dom::create_div().with_css(
        "width: 1px; height: 20px; background: system:separator; margin: 0px 4px; flex-shrink: 0;",
    )
}

// TODO(WIDGETS9A): Toolbar - this row becomes the shared Toolbar widget
// (overflow into a "more" menu when the window is narrow).
fn toolbar(s: &AppState, data: &RefAny) -> Dom {
    let count = s.doc.as_ref().map_or(0, crate::jobs::Doc::page_count);
    let page = s.current_page.min(count.saturating_sub(1));
    let mut bar = Dom::create_div().with_id(ids::TOOLBAR).with_css(BAR);
    bar.add_child(
        Button::create("Open")
            .with_icon("folder_open")
            .with_on_click(data.clone(), crate::on_open as ButtonOnClickCallbackType)
            .dom()
            .with_id(ids::OPEN),
    );
    bar.add_child(separator());

    let mut prev = icon_button("chevron_left", data, crate::on_prev);
    if page == 0 {
        prev = prev.with_disabled("This is the first page");
    }
    bar.add_child(
        prev.dom()
            .with_id(ids::PREV)
            .with_accessibility_name("Previous page"),
    );
    bar.add_child(
        Dom::create_div()
            .with_css("width: 56px; flex-shrink: 0;")
            .with_child(
                TextInput::create()
                    .with_text(format!("{}", page + 1).as_str())
                    .with_accessibility_name("Page")
                    .with_on_virtual_key_down(
                        data.clone(),
                        crate::on_page_field_key as TextInputOnVirtualKeyDownCallbackType,
                    )
                    .dom()
                    .with_id(ids::PAGE_FIELD),
            ),
    );
    bar.add_child(
        Dom::create_span_with_text(format!("/ {count}").as_str())
            .with_id(ids::PAGE_COUNT)
            .with_css("font-size: 12px; color: system:secondary-text; flex-shrink: 0;"),
    );
    let mut next = icon_button("chevron_right", data, crate::on_next);
    if page + 1 >= count {
        next = next.with_disabled("This is the last page");
    }
    bar.add_child(
        next.dom()
            .with_id(ids::NEXT)
            .with_accessibility_name("Next page"),
    );
    bar.add_child(separator());

    bar.add_child(
        icon_button("remove", data, crate::on_zoom_out)
            .dom()
            .with_id(ids::ZOOM_OUT)
            .with_accessibility_name("Zoom out"),
    );
    let choices = Zoom::choices();
    let labels: Vec<String> = choices.iter().map(|z| z.label()).collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let selected = choices.iter().position(|z| *z == s.zoom).unwrap_or(0);
    bar.add_child(
        Dom::create_div()
            .with_css("width: 110px; flex-shrink: 0;")
            .with_child(
                DropDown::create(strs(&label_refs))
                    .with_selected(selected)
                    .with_accessibility_name("Zoom")
                    .with_on_choice_change(
                        data.clone(),
                        crate::on_zoom_choice as DropDownOnChoiceChangeCallbackType,
                    )
                    .dom()
                    .with_id(ids::ZOOM),
            ),
    );
    bar.add_child(
        icon_button("add", data, crate::on_zoom_in)
            .dom()
            .with_id(ids::ZOOM_IN)
            .with_accessibility_name("Zoom in"),
    );

    bar.add_child(Dom::create_div().with_css("flex-grow: 1;"));
    bar.add_child(
        Dom::create_div()
            .with_css("width: 220px; flex-shrink: 1;")
            .with_child(
                TextInput::create_search()
                    .with_text(s.search.query.as_str())
                    .with_placeholder("Find in document")
                    .with_accessibility_name("Find in document")
                    .with_on_virtual_key_down(
                        data.clone(),
                        crate::on_search_key as TextInputOnVirtualKeyDownCallbackType,
                    )
                    .dom()
                    .with_id(ids::SEARCH_FIELD),
            ),
    );
    bar.add_child(
        icon_button("settings", data, crate::on_settings_open)
            .dom()
            .with_id(ids::SETTINGS)
            .with_accessibility_name("Settings"),
    );
    bar
}

// ==== The page view ====

/// What a VirtualView of AzPdf's hands its callback: the app.
struct ViewData {
    app: RefAny,
}

fn page_view(data: &RefAny) -> Dom {
    Dom::create_virtual_view(RefAny::new(ViewData { app: data.clone() }), pages_view)
        .with_id(ids::PAGES)
        .with_accessibility_name("Pages")
        .with_css(
            "flex-grow: 1; min-height: 0px; width: 100%; background: \
             system:under-page-background;",
        )
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::create(LogicalPosition::create(x, y), LogicalSize::create(w, h))
}

/// The materialized slice of a strip: pages `first..end` and the slice's
/// top and bottom (each page owns half of the gaps around it).
fn slice(strip: &Strip, first: usize, end: usize) -> (f32, f32) {
    let n = strip.len();
    let top = if first == 0 {
        0.0
    } else {
        strip.tops[first] - PAGE_GAP / 2.0
    };
    let bottom = if end >= n {
        strip.height
    } else {
        strip.tops[end - 1] + strip.sizes[end - 1].1 + PAGE_GAP / 2.0
    };
    (top, bottom.max(top + 1.0))
}

/// The page view: the pages in view and one either side, centred, each the
/// picture azul drew (the sharp one, else the nearest one meanwhile), or a
/// blank page with its number.
extern "C" fn pages_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let mut app = match data.downcast_ref::<ViewData>() {
        Some(view) => view.app.clone(),
        None => return VirtualViewReturn::default(),
    };
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return VirtualViewReturn::default();
    };
    let s = &mut *guard;
    let logical = info.bounds.get_logical_size();
    let physical = info.bounds.get_physical_size();
    if logical.width > 0.0 {
        s.dpi = (physical.width as f32 / logical.width).max(1.0);
    }
    s.view = (logical.width.max(1.0), logical.height.max(1.0));
    s.view_generation = s.generation;

    let strip = s.strip();
    let n = strip.len();
    let view_w = s.view.0.max(strip.width);
    if n == 0 {
        return VirtualViewReturn::with_dom(
            Dom::create_div(),
            rect(0.0, 0.0, view_w, 1.0),
            rect(0.0, 0.0, view_w, 1.0),
        );
    }
    let y = info.scroll_offset.y.max(0.0);
    let visible = strip.visible(y, s.view.1);
    let first = visible.start.saturating_sub(1).min(n - 1);
    let end = (visible.end + 1).min(n).max(first + 1);
    s.visible = visible;
    s.current_page = strip.page_at(y + s.view.1 / 3.0);

    let (top, bottom) = slice(&strip, first, end);
    let mut root = Dom::create_div().with_css(
        format!(
            "position: relative; width: {view_w}px; height: {}px;",
            bottom - top
        )
        .as_str(),
    );
    for page in first..end {
        let (w, h) = strip.sizes[page];
        let x = ((view_w - w) / 2.0).max(VIEW_PAD);
        let width = s.page_render_width(page);
        let image = s
            .pages
            .get(page, width)
            .or_else(|| s.pages.nearest(page, width));
        root.add_child(page_frame(page, x, strip.tops[page] - top, w, h, image));
    }
    VirtualViewReturn::with_dom(
        root,
        rect(0.0, top, view_w, bottom - top),
        rect(0.0, 0.0, view_w, strip.height),
    )
}

/// One page at (`x`, `y`) of the slice, `w` x `h` CSS px.
fn page_frame(page: usize, x: f32, y: f32, w: f32, h: f32, image: Option<ImageRef>) -> Dom {
    let mut frame = Dom::create_div()
        .with_id(ids::numbered(ids::PAGE_PREFIX, page + 1))
        .with_accessibility_name(format!("Page {}", page + 1).as_str())
        .with_css(
            format!(
                "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; \
                 background: #ffffff; box-shadow: 0px 1px 4px #00000040; overflow: hidden;"
            )
            .as_str(),
        );
    match image {
        Some(image) => frame.add_child(
            Dom::create_image(image).with_css(format!("width: {w}px; height: {h}px;").as_str()),
        ),
        None => frame.add_child(
            Dom::create_div()
                .with_css(
                    "display: flex; align-items: center; justify-content: center; width: 100%; \
                     height: 100%; color: #9a9a9a; font-size: 13px;",
                )
                .with_child(Dom::create_span_with_text(
                    format!("Page {}", page + 1).as_str(),
                )),
        ),
    }
    frame
}

// ==== The navigation pane: thumbnails or the outline ====

fn navigation(s: &AppState, data: &RefAny) -> Dom {
    let mut column = Dom::create_div().with_css(COLUMN);
    column.add_child(
        Dom::create_div()
            .with_css("padding: 6px 8px; flex-shrink: 0;")
            .with_child(
                Segmented::create(strs(&["Pages", "Outline"]))
                    .with_selected_index(usize::from(s.nav == Nav::Outline))
                    .with_on_change(
                        data.clone(),
                        crate::on_nav_tab as SegmentedOnChangeCallbackType,
                    )
                    .dom()
                    .with_id(ids::NAV_TABS),
            ),
    );
    match s.nav {
        Nav::Pages => column.add_child(
            Dom::create_virtual_view(RefAny::new(ViewData { app: data.clone() }), thumbs_view)
                .with_id(ids::THUMBS)
                .with_accessibility_name("Page thumbnails")
                .with_css("flex-grow: 1; min-height: 0px; width: 100%;"),
        ),
        Nav::Outline => column.add_child(outline(s, data)),
    }
    column
}

/// The thumbnail rail: like the page view, at the thumbnails' scale; the
/// current page is marked, a click goes to the page.
extern "C" fn thumbs_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let mut app = match data.downcast_ref::<ViewData>() {
        Some(view) => view.app.clone(),
        None => return VirtualViewReturn::default(),
    };
    let cb_app = app.clone();
    let Some(mut guard) = app.downcast_mut::<AppState>() else {
        return VirtualViewReturn::default();
    };
    let s = &mut *guard;
    let sizes = s.doc.as_ref().map_or_else(Vec::new, |d| d.sizes.clone());
    let strip = Strip::new(&sizes, s.thumb_scale());
    let n = strip.len();
    let logical = info.bounds.get_logical_size();
    let view_w = logical.width.max(strip.width);
    if n == 0 {
        return VirtualViewReturn::with_dom(
            Dom::create_div(),
            rect(0.0, 0.0, view_w, 1.0),
            rect(0.0, 0.0, view_w, 1.0),
        );
    }
    let y = info.scroll_offset.y.max(0.0);
    let visible = strip.visible(y, logical.height.max(1.0));
    let first = visible.start.saturating_sub(2).min(n - 1);
    let end = (visible.end + 2).min(n).max(first + 1);
    s.thumbs_visible = first..end;

    let (top, bottom) = slice(&strip, first, end);
    let mut root = Dom::create_div().with_css(
        format!(
            "position: relative; width: {view_w}px; height: {}px;",
            bottom - top
        )
        .as_str(),
    );
    for page in first..end {
        let (w, h) = strip.sizes[page];
        let x = ((view_w - w) / 2.0).max(0.0);
        let width = s.thumb_render_width(page);
        let image = s
            .thumbs
            .get(page, width)
            .or_else(|| s.thumbs.nearest(page, width));
        let current = page == s.current_page;
        root.add_child(thumb_frame(
            page,
            (x, strip.tops[page] - top, w, h),
            image,
            current,
            &cb_app,
        ));
    }
    VirtualViewReturn::with_dom(
        root,
        rect(0.0, top, view_w, bottom - top),
        rect(0.0, 0.0, view_w, strip.height),
    )
}

fn thumb_frame(
    page: usize,
    (x, y, w, h): (f32, f32, f32, f32),
    image: Option<ImageRef>,
    current: bool,
    app: &RefAny,
) -> Dom {
    let border = if current {
        "2px solid system:accent"
    } else {
        "1px solid system:separator"
    };
    let mut frame = Dom::create_div()
        .with_id(ids::numbered(ids::THUMB_PREFIX, page + 1))
        .with_accessibility_name(format!("Page {}", page + 1).as_str())
        .with_dataset(tag(page))
        .with_css(
            format!(
                "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; \
                 background: #ffffff; border: {border}; box-sizing: border-box; overflow: \
                 hidden;"
            )
            .as_str(),
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            app.clone(),
            crate::on_go_to_page,
        );
    if let Some(image) = image {
        frame.add_child(Dom::create_image(image).with_css("width: 100%; height: 100%;"));
    }
    frame.add_child(
        Dom::create_span_with_text(format!("{}", page + 1).as_str()).with_css(
            "position: absolute; right: 4px; bottom: 4px; padding: 1px 5px; font-size: 10px; \
             border-radius: 3px; background: #00000099; color: #ffffff;",
        ),
    );
    frame
}

fn outline(s: &AppState, data: &RefAny) -> Dom {
    let mut list = Dom::create_div()
        .with_id(ids::OUTLINE)
        .with_accessibility_name("Outline")
        .with_css("flex-grow: 1; min-height: 0px; overflow-y: auto; padding: 4px 0px;");
    let entries = s.doc.as_ref().map_or(&[][..], |d| d.outline.as_slice());
    if entries.is_empty() {
        list.add_child(
            Dom::create_p_with_text("This document has no outline.").with_css(
                "margin: 0px; padding: 12px; font-size: 12px; color: system:secondary-text;",
            ),
        );
        return list;
    }
    for (i, (title, page)) in entries.iter().enumerate() {
        let mut row = Dom::create_div()
            .with_id(ids::numbered(ids::OUTLINE_PREFIX, i))
            .with_dataset(tag(*page))
            .with_css(
                "display: flex; flex-direction: row; gap: 8px; padding: 4px 12px; font-size: \
                 12px;",
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                data.clone(),
                crate::on_go_to_page,
            );
        row.add_child(
            Dom::create_span_with_text(title.as_str())
                .with_css("flex-grow: 1; min-width: 0px; overflow: hidden;"),
        );
        row.add_child(
            Dom::create_span_with_text(format!("{}", page + 1).as_str())
                .with_css("color: system:secondary-text; flex-shrink: 0;"),
        );
        list.add_child(row);
    }
    list
}

// ==== The side pane: the search hits ====

fn hits_pane(s: &AppState, data: &RefAny) -> Dom {
    let mut column = Dom::create_div().with_id(ids::HITS).with_css(COLUMN);
    let heading = if s.search.running {
        "Searching\u{2026}".to_string()
    } else {
        match s.search.hits.len() {
            0 => "No matches".to_string(),
            1 => "1 match".to_string(),
            n => format!("{n} matches"),
        }
    };
    column.add_child(
        Dom::create_div()
            .with_css(BAR)
            .with_child(
                Dom::create_span_with_text(heading.as_str())
                    .with_css("flex-grow: 1; font-size: 12px; font-weight: 600;"),
            )
            .with_child(
                Button::create("")
                    .with_icon("close")
                    .with_on_click(
                        data.clone(),
                        crate::on_search_close as ButtonOnClickCallbackType,
                    )
                    .dom()
                    .with_accessibility_name("Close the search"),
            ),
    );
    let mut list = Dom::create_div().with_css("flex-grow: 1; min-height: 0px; overflow-y: auto;");
    for (i, hit) in s.search.hits.iter().take(MAX_HIT_ROWS).enumerate() {
        let mut row = Dom::create_div()
            .with_id(ids::numbered(ids::HIT_PREFIX, i))
            .with_dataset(tag(hit.page))
            .with_css(
                "display: flex; flex-direction: column; gap: 2px; padding: 6px 12px; \
                 border-bottom: 1px solid system:separator;",
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                data.clone(),
                crate::on_go_to_page,
            );
        row.add_child(
            Dom::create_span_with_text(format!("Page {}", hit.page + 1).as_str())
                .with_css("font-size: 11px; color: system:secondary-text;"),
        );
        row.add_child(
            Dom::create_span_with_text(hit.snippet.as_str()).with_css("font-size: 12px;"),
        );
        list.add_child(row);
    }
    if s.search.hits.len() > MAX_HIT_ROWS {
        list.add_child(
            Dom::create_p_with_text(
                format!("\u{2026} and {} more", s.search.hits.len() - MAX_HIT_ROWS).as_str(),
            )
            .with_css("margin: 0px; padding: 8px 12px; font-size: 12px;"),
        );
    }
    column.add_child(list);
    column
}

// ==== The status bar ====

fn status_bar(s: &AppState) -> Dom {
    let mut segments = Vec::new();
    if let Some(doc) = s.doc.as_ref() {
        let count = doc.page_count();
        let page = s.current_page.min(count.saturating_sub(1));
        segments.push(StatusBarSegment::create(
            format!("Page {} of {count}", page + 1).as_str(),
        ));
        if let Some(size) = doc.sizes.get(page) {
            segments.push(StatusBarSegment::create(size_label(*size).as_str()));
        }
        let percent = (s.scale() * 100.0).round() as u32;
        segments.push(StatusBarSegment::create(
            match s.zoom {
                Zoom::Percent(_) => format!("{percent} %"),
                other => format!("{} ({percent} %)", other.label()),
            }
            .as_str(),
        ));
    }
    segments.push(StatusBarSegment::create(s.status.as_str()));
    StatusBar::create(segments).dom()
}
