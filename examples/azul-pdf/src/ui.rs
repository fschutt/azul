//! AzPdf's window: the start screen (the empty state and the recent
//! documents) or the document shell - the navigation pane (Pages / Outline),
//! the toolbar over the page view, the search hits in the side pane, the
//! status bar. The page view and the thumbnail rail are VirtualViews: only
//! the pages in view (and one either side) exist in the DOM - in the view
//! each page's own DOM (its SVG read as markup: shapes, and text that is
//! text), in the rail a picture of it - or a blank page with its number while
//! it is being made. The pages are paper: white in both modes.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType,
        DropDownOnChoiceChangeCallbackType, SegmentedOnChangeCallbackType,
        TextInputOnTextInputCallbackType, TextInputOnVirtualKeyDownCallbackType,
        ToolbarOnEventCallbackType,
    },
    css::{Css, CssDeclaration, CssPropertyWithConditions},
    image::ImageRef,
    prelude::*,
    shells::{DocumentShell, ShellEmptyState, ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    vec::{CssPropertyWithConditionsVec, StringVec},
    widgets::{
        CheckBox, CheckBoxState, DropDown, OnTextInputReturn, Segmented, StatusBar,
        StatusBarSegment, TextInputState, TextInputValid, Toolbar, ToolbarEvent, ToolbarEventKind,
        ToolbarItem,
    },
};
use azul_appkit::ui as kit;

use crate::{
    ids,
    model::{file_title, size_label, Field, FieldKind, FieldWidget, Strip, Zoom, PAGE_GAP, VIEW_PAD},
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
         solid system:separator; border-radius: 6px; @theme(flora) { border-radius: 5px; }",
    );
    // Under flora the list's heading is flora's label (capitals in the label ink).
    list.add_child(Dom::create_h2_with_text("Recent").with_css(
        "font-size: 13px; font-weight: 600; margin: 0px; padding: 8px 12px; border-bottom: 1px \
         solid system:separator; @theme(flora) { font-size: 11px; font-weight: bold; \
         text-transform: uppercase; letter-spacing: 0.12em; color: system:secondary-text; }",
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

/// The toolbar: azul's `Toolbar` - Open, the page navigation (the page field
/// and the page count are embedded controls), the zoom, the search field
/// (never in the "more" menu) and the settings gear. Each tool's `id` is its
/// DOM-id name from [`ids`]: what [`on_toolbar`] matches.
fn toolbar(s: &AppState, data: &RefAny) -> Dom {
    let count = s.doc.as_ref().map_or(0, crate::jobs::Doc::page_count);
    let page = s.current_page.min(count.saturating_sub(1));

    let mut prev = ToolbarItem::create_button(ids::PREV, "Previous page", "chevron_left");
    if page == 0 {
        prev = prev.with_disabled("This is the first page");
    }
    let mut next = ToolbarItem::create_button(ids::NEXT, "Next page", "chevron_right");
    if page + 1 >= count {
        next = next.with_disabled("This is the last page");
    }
    let page_field = TextInput::create()
        .with_text(format!("{}", page + 1).as_str())
        .with_accessibility_name("Page")
        .with_on_virtual_key_down(
            data.clone(),
            crate::on_page_field_key as TextInputOnVirtualKeyDownCallbackType,
        )
        .dom()
        .with_id(ids::PAGE_FIELD);
    let page_count = Dom::create_span_with_text(format!("/ {count}").as_str())
        .with_id(ids::PAGE_COUNT)
        .with_css("font-size: 12px; color: system:secondary-text;");

    let choices = Zoom::choices();
    let labels: Vec<String> = choices.iter().map(|z| z.label()).collect();
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let selected = choices.iter().position(|z| *z == s.zoom).unwrap_or(0);
    let zoom = DropDown::create(strs(&label_refs))
        .with_selected(selected)
        .with_accessibility_name("Zoom")
        .with_on_choice_change(
            data.clone(),
            crate::on_zoom_choice as DropDownOnChoiceChangeCallbackType,
        )
        .dom()
        .with_id(ids::ZOOM);
    let search = TextInput::create_search()
        .with_text(s.search.query.as_str())
        .with_placeholder("Find in document")
        .with_accessibility_name("Find in document")
        .with_on_virtual_key_down(
            data.clone(),
            crate::on_search_key as TextInputOnVirtualKeyDownCallbackType,
        )
        .dom()
        .with_id(ids::SEARCH_FIELD);

    let has_form = s.doc.as_ref().is_some_and(|d| !d.fields.is_empty());
    let mut items = vec![
        ToolbarItem::create_button(ids::OPEN, "Open", "folder_open").with_show_label(true),
        ToolbarItem::create_separator(),
        prev,
        ToolbarItem::create_custom(ids::PAGE_FIELD, "Page", page_field, 56.0),
        ToolbarItem::create_custom(ids::PAGE_COUNT, "Page count", page_count, 40.0),
        next,
        ToolbarItem::create_separator(),
        ToolbarItem::create_button(ids::ZOOM_OUT, "Zoom out", "remove"),
        ToolbarItem::create_custom(ids::ZOOM, "Zoom", zoom, 110.0),
        ToolbarItem::create_button(ids::ZOOM_IN, "Zoom in", "add"),
        ToolbarItem::create_spacer(),
        ToolbarItem::create_custom(ids::SEARCH_FIELD, "Find in document", search, 220.0)
            .with_never_overflow(true),
        ToolbarItem::create_button(ids::SETTINGS, "Settings", "settings"),
    ];
    if has_form {
        // After Open: the document's own command.
        items.insert(
            1,
            ToolbarItem::create_button(ids::EXPORT_FILLED, "Export filled PDF", "download")
                .with_show_label(true),
        );
    }
    Toolbar::create("Document")
        .with_items(items)
        .with_on_event(data.clone(), on_toolbar as ToolbarOnEventCallbackType)
        .dom()
        .with_id(ids::TOOLBAR)
}

/// A tool was pressed: the tool's `id` names the command.
extern "C" fn on_toolbar(data: RefAny, info: CallbackInfo, event: ToolbarEvent) -> Update {
    if event.kind != ToolbarEventKind::Activate {
        return Update::DoNothing;
    }
    let id = event.id.as_str();
    if id == ids::OPEN.as_str() {
        crate::on_open(data, info)
    } else if id == ids::PREV.as_str() {
        crate::on_prev(data, info)
    } else if id == ids::NEXT.as_str() {
        crate::on_next(data, info)
    } else if id == ids::ZOOM_OUT.as_str() {
        crate::on_zoom_out(data, info)
    } else if id == ids::ZOOM_IN.as_str() {
        crate::on_zoom_in(data, info)
    } else if id == ids::SETTINGS.as_str() {
        crate::on_settings_open(data, info)
    } else if id == ids::EXPORT_FILLED.as_str() {
        crate::on_export_filled(data, info)
    } else {
        Update::DoNothing
    }
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

/// The page view: the pages in view and one either side, centred, each its
/// DOM, or a blank page with its number while it is made.
extern "C" fn pages_view(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let mut app = match data.downcast_ref::<ViewData>() {
        Some(view) => view.app.clone(),
        None => return VirtualViewReturn::default(),
    };
    let cb_app = app.clone();
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
        let dom = s.pages.get(page, crate::PAGE_DOM);
        let mut frame = page_frame(page, x, strip.tops[page] - top, w, h, dom);
        // The form's inputs over the page, at the page's scale.
        if let Some(doc) = s.doc.as_ref() {
            let px_per_pt = doc.sizes.get(page).map_or(0.0, |p| w / p.width_pt.max(1.0));
            for (fi, field) in doc.fields.iter().enumerate() {
                for (wi, widget) in field.widgets.iter().enumerate() {
                    if widget.page == page {
                        frame.add_child(field_input(
                            (fi, wi),
                            field,
                            widget,
                            s.form.get(&field.name),
                            px_per_pt,
                            &cb_app,
                        ));
                    }
                }
            }
        }
        root.add_child(frame);
    }
    VirtualViewReturn::with_dom(
        root,
        rect(0.0, top, view_w, bottom - top),
        rect(0.0, 0.0, view_w, strip.height),
    )
}

/// One page at (`x`, `y`) of the slice, `w` x `h` CSS px: its DOM (an
/// `<svg>`, sized to the frame - its viewBox maps the page onto it).
fn page_frame(page: usize, x: f32, y: f32, w: f32, h: f32, dom: Option<Dom>) -> Dom {
    let mut frame = Dom::create_div()
        .with_id(ids::numbered(ids::PAGE_PREFIX, page + 1))
        .with_accessibility_name(format!("Page {}", page + 1).as_str())
        .with_css(
            format!(
                "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; \
                 background: #ffffff; box-shadow: 0px 1px 4px #00000040; overflow: hidden; \
                 @theme(flora) {{ box-shadow: 0px 1px 4px rgba(48, 45, 38, 0.3); @media \
                 (prefers-color-scheme: dark) {{ box-shadow: 0px 1px 4px rgba(0, 0, 0, 0.55); }} }}"
            )
            .as_str(),
        );
    match dom {
        Some(dom) => frame.add_child(
            dom.with_css(format!("display: block; width: {w}px; height: {h}px;").as_str()),
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

// ==== The form: an input over each field ====

/// What a field's input hands its callback: the app, the field and widget.
struct FieldRef {
    app: RefAny,
    field: usize,
    widget: usize,
}

/// The input of `field`'s `widget` over its page, `px_per_pt` CSS px per
/// point: a text box, a check box (a radio button is one too) or a drop-down
/// in a box at the field's place, tinted like a form field; a read-only
/// field shows its value as text. The box places it: a widget's own style
/// outranks CSS given to its DOM, so the widget only fills the box.
fn field_input(
    (fi, wi): (usize, usize),
    field: &Field,
    widget: &FieldWidget,
    value: &str,
    px_per_pt: f32,
    app: &RefAny,
) -> Dom {
    let (x, y, w, h) = widget.rect;
    let place = format!(
        "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; \
         box-sizing: border-box; margin: 0px;",
        x * px_per_pt,
        y * px_per_pt,
        w * px_per_pt,
        h * px_per_pt
    );
    let font = format!("font-size: {}px;", field.font_px(widget, px_per_pt));
    let id = AzString::from(format!("{}{fi}-{wi}", ids::FIELD_PREFIX));
    let data = || {
        RefAny::new(FieldRef {
            app: app.clone(),
            field: fi,
            widget: wi,
        })
    };
    let boxed = |input: Dom| {
        Dom::create_div()
            .with_id(id.clone())
            .with_css(format!("{place} display: flex; background: #dbe6fbcc;").as_str())
            .with_child(input.with_css("flex-grow: 1; width: 100%; height: 100%; min-height: 0px;"))
    };
    if field.read_only {
        return Dom::create_span_with_text(value)
            .with_id(id)
            .with_css(format!("{place} {font} color: #000000; overflow: hidden;").as_str());
    }
    let input = match field.kind {
        FieldKind::Text => {
            let input = if field.password {
                TextInput::create_password()
            } else {
                TextInput::create()
            };
            // On paper in either mode: the field's tint shows through, the
            // ink is black (the input's own look follows the dark mode).
            input
                .with_text(value)
                .with_accessibility_name(field.name.as_str())
                .with_container_style(paper_style(&format!(
                    "position: relative; cursor: text; box-sizing: border-box; \
                     min-height: 0px; flex-grow: 1; background: transparent; color: #000000; \
                     padding: 0px 3px; border: none; {font}"
                )))
                .with_label_style(paper_style(
                    "display: block; flex-grow: 0; position: relative; overflow-x: auto; \
                     overflow-y: hidden; scrollbar-width: none; white-space: pre; color: #000000;",
                ))
                .with_on_text_input(data(), on_field_text as TextInputOnTextInputCallbackType)
                .dom()
                .with_css(font.as_str())
        }
        FieldKind::CheckBox | FieldKind::Radio => {
            CheckBox::create(Field::is_checked(value, widget))
                .with_accessibility_name(field.name.as_str())
                .with_on_toggle(data(), on_field_toggle as CheckBoxOnToggleCallbackType)
                .dom()
        }
        FieldKind::Choice => {
            let labels: Vec<&str> = field.options.iter().map(String::as_str).collect();
            let selected = field.options.iter().position(|o| o == value).unwrap_or(0);
            DropDown::create(strs(&labels))
                .with_selected(selected)
                .with_accessibility_name(field.name.as_str())
                .with_on_choice_change(data(), on_field_choice as DropDownOnChoiceChangeCallbackType)
                .dom()
                .with_css(font.as_str())
        }
        FieldKind::Other => return Dom::create_div().with_id(id).with_css(place.as_str()),
    };
    boxed(input)
}

/// The declarations of the inline CSS `css` as a widget's part style that
/// looks the same in either mode: each one again as its own dark twin, so the
/// theme adds none of its own.
fn paper_style(css: &str) -> CssPropertyWithConditionsVec {
    let parsed = Css::parse_inline(css);
    let mut out = Vec::new();
    for rule in parsed.rules.as_ref() {
        for declaration in rule.declarations.as_ref() {
            if let CssDeclaration::Static(property) = declaration {
                out.push(CssPropertyWithConditions::simple(property.clone()));
                out.push(CssPropertyWithConditions::dark_mode(property.clone()));
            }
        }
    }
    out.into()
}

/// Sets field `fi`'s value; `widget` picks a check box's / radio button's
/// on-state. Prints `AZPDF_FIELD <name>=<value>` when that changed it.
fn set_field(data: &mut RefAny, set: impl FnOnce(&Field, &FieldWidget) -> Option<String>) -> bool {
    let Some((mut app, fi, wi)) = data
        .downcast_ref::<FieldRef>()
        .map(|r| (r.app.clone(), r.field, r.widget))
    else {
        return false;
    };
    let Some(mut s) = app.downcast_mut::<AppState>() else {
        return false;
    };
    let Some((name, value)) = s.doc.as_ref().and_then(|d| {
        let field = d.fields.get(fi)?;
        let value = set(field, field.widgets.get(wi)?)?;
        Some((field.name.clone(), value))
    }) else {
        return false;
    };
    let changed = s.form.set(&name, &value);
    if changed {
        println!("AZPDF_FIELD {name}={value}");
    }
    changed
}

extern "C" fn on_field_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().to_string();
    set_field(&mut data, |_, _| Some(text));
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_field_toggle(mut data: RefAny, _info: CallbackInfo, state: CheckBoxState) -> Update {
    let changed = set_field(&mut data, |field, widget| match (field.kind, state.checked) {
        (_, true) => Some(Field::on_value(widget)),
        (FieldKind::CheckBox, false) => Some("Off".to_string()),
        // A radio button is unchecked by checking another one.
        _ => None,
    });
    if changed {
        // A radio group's other buttons follow.
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

extern "C" fn on_field_choice(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    set_field(&mut data, |field, _| field.options.get(index).cloned());
    Update::DoNothing
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
             border-radius: 3px; background: #00000099; color: #ffffff; @theme(flora) { \
             font-size: 12px; background: rgba(38, 37, 33, 0.72); color: #F4F2EA; }",
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
