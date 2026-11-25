use azul::{
    audio::AudioConfig,
    callbacks::{ButtonOnClickCallbackType, CallbackType},
    dom::{
        AccessibilityInfo, AccessibilityRole, DomNodeId, NodeId, RenderImageCallback, VirtualKeyCode,
        VirtualKeyCodeCombo,
    },
    option::OptionVirtualKeyCodeCombo,
    image::ImageRef,
    menu::{Menu, MenuItem, StringMenuItem},
    prelude::*,
    shells::{ShellThemeAccent, ShellThemeScope},
    str::String as AzString,
    vec::MenuItemVec,
    widgets::{AlertKind, Button, InfoBar, LevelMeter, MicrophoneWidget},
};
use azul_appkit::{
    l10n::{label, t, t_args, Arg},
    ui as kit,
};

use crate::{
    code, column, ids,
    ink::{self, InkCanvas, Layer},
    model::{Note, Semantic},
    voice, AppState,
};

/// A column that takes the rest of its parent.
const COLUMN: &str = "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                      min-width: 0px;";

pub const LINE_H: f32 = 15.0;
const PAGE_W: f32 = 1000.0;
const GUTTER_W: f32 = 52.0;

const MARGIN_GUTTER_W: f32 = 260.0;
pub const PAGE_PAD: f32 = 32.0;
const PAGE_GAP: f32 = 24.0;

pub const METER_PACKET_SAMPLES: usize = 4096;


/// The line of a page's code (0 for its first) under `y`, in the page's own coordinates: the code
/// starts under the page's padding.
#[must_use]
pub fn line_of(y: f32) -> usize {
    ((y - PAGE_PAD) / LINE_H).floor().max(0.0) as usize
}

fn page_h() -> f32 {
    code::LINES_PER_PAGE as f32 * LINE_H + 40.0
}

fn page_stride() -> f32 {
    PAGE_W + PAGE_GAP
}

/// The page of the ink layer under the pointer (its dataset is its canvas).
pub fn page_of(info: &mut CallbackInfo) -> Option<usize> {
    info.get_dataset(info.get_hit_node())
        .into_option()
        .and_then(|mut d| d.downcast_ref::<InkCanvas>().map(|c| c.page))
}

/// Asks for a new frame of the page's `layer` under the pointer, no new DOM:
/// the pen layer is the node the pointer handlers sit on, the marker layer
/// the first child of the same sheet.
pub fn redraw_ink(info: &mut CallbackInfo, layer: Layer) {
    let pen = info.get_hit_node();
    let node = match layer {
        Layer::Pen => Some(pen),
        Layer::Marker => info
            .get_parent(pen)
            .into_option()
            .and_then(|sheet| info.get_first_child(sheet).into_option()),
    };
    if let Some(node) = node {
        kit::redraw_image(info, node);
    }
}

/// The pointer as a stroke sees it.
#[derive(Debug, Clone, Copy)]
pub struct Sampled {
    /// Where, in the pen layer's px, with the pen's pressure and tilt (a mouse's are the defaults).
    pub point: crate::model::InkPoint,
    /// The pen's eraser end is down.
    pub eraser: bool,
    /// A button or the pen's tip is down, as the window says: the button is the tip's too on a pen.
    pub pressed: bool,
    /// A pen drives the pointer.
    pub pen: bool,
    /// The pen's pressure while its tip is down (0 for a mouse or a trackpad, and for a pen in the air).
    pub pressure: f32,
}

pub fn sample(info: &mut CallbackInfo) -> Option<Sampled> {
    let p = info.get_cursor_relative_to_node().into_option()?;
    let button = info.get_current_mouse_state().left_down;
    let pen = info.get_pen_state().into_option();
    if let Some(pen) = &pen {
        if pen.in_contact {
            return Some(Sampled {
                point: ink::point_from(p.x, p.y, pen.pressure, pen.tilt.x_tilt, pen.tilt.y_tilt),
                eraser: pen.is_eraser,
                pressed: true,
                pen: true,
                pressure: pen.pressure,
            });
        }
    }
    Some(Sampled {
        point: ink::point_from(p.x, p.y, 0.0, 0.0, 0.0),
        eraser: false,
        pressed: button,
        pen: pen.is_some(),
        pressure: 0.0,
    })
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

/// A control of the bars - a meaning, the nib, a page, a tool, the files'
/// switch: a button of its own, 22px tall, without the UA's face (its
/// padding, border and 13px font), pressed and not dragged in the title row.
pub(crate) const KEY: &str = "display: flex; flex-direction: row; align-items: center; justify-content: \
                   center; gap: 4px; height: 22px; box-sizing: border-box; padding: 0px 6px; \
                   border-radius: 4px; font-size: 11px; flex-shrink: 0; cursor: pointer; \
                   -azul-app-region: no-drag;";

/// An icon-only key at rest: no face until the pointer is on it.
pub(crate) const QUIET: &str = "padding: 0px 3px; background: transparent; border: 1px solid transparent; \
                     color: system:text; :hover { background: system:control-background; \
                     border: 1px solid system:separator; }";

/// A key (`KEY` and `face`) named `name` for the screen reader, clicked (or
/// pressed with Enter / Space) for `on_click` with `data`.
pub(crate) fn key(face: &str, name: AzString, data: RefAny, on_click: CallbackType) -> Dom {
    Dom::create_node(NodeType::Button)
        .with_css(format!("{KEY} {face}").as_str())
        .with_accessibility_name(name)
        .with_callback(EventFilter::Hover(HoverEventFilter::Click), data, on_click)
}

/// What an icon-only key shows under the pointer: its name.
struct Tip(AzString);

/// `key` with its name as the tooltip while the pointer is on it.
pub(crate) fn with_tip(key: Dom, name: &AzString) -> Dom {
    key.with_callback(
        EventFilter::Hover(HoverEventFilter::MouseEnter),
        RefAny::new(Tip(name.clone())),
        show_tip,
    )
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseLeave), RefAny::new(()), hide_tip)
}

extern "C" fn show_tip(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(tip) = data.downcast_ref::<Tip>().map(|t| t.0.clone()) {
        info.show_tooltip(tip);
    }
    Update::DoNothing
}

extern "C" fn hide_tip(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.hide_tooltip();
    Update::DoNothing
}

pub extern "C" fn layout(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    azul_appkit::l10n::begin_layout(&info);
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
            .with_child(kit::settings_page_with_reload(
                &s.kit,
                vec![column::settings_section(&s, &data)],
                &data,
                column::reload_settings,
            ))
    } else {
        main_view(&s, &data)
    };
    // The scope as the window's body: no UA margin, the full window height,
    // the app theme's ground and ink in both modes.
    ShellThemeScope::create(Dom::create_div().with_css(COLUMN).with_child(content))
        .with_accent(ShellThemeAccent::Slate)
        .body()
        .with_menu_bar(menu_bar(&data))
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            data.clone(),
            crate::on_key,
        )
        // A stroke ends with the release wherever it is, with the pointer leaving the window, with the
        // window losing its focus: not only with a release over the page's pen layer.
        .with_callback(
            EventFilter::Window(WindowEventFilter::LeftMouseUp),
            data.clone(),
            crate::on_window_up,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::MouseLeave),
            data.clone(),
            crate::on_window_left,
        )
        .with_callback(
            EventFilter::Window(WindowEventFilter::FocusLost),
            data.clone(),
            crate::on_focus_lost,
        )
}

/// The review window: the title row (it moves the window), the page rail,
/// and the pages - what went wrong over them, and the files flying out over
/// them while the switch has them out.
pub fn main_view(s: &AppState, data: &RefAny) -> Dom {
    let mut pages = Dom::create_div().with_css(
        "position: relative; display: flex; flex-direction: column; flex-grow: 1; min-height: \
         0px; min-width: 0px;",
    );
    if let Some(problem) = s.problem.as_deref() {
        pages.add_child(problem_bar(problem, data));
    }
    pages.add_child(sheet(s, data));
    if s.sidebar_open {
        pages.add_child(flyout(s, data));
    }
    // The pages and, beside them, the side column of dictated notes.
    let mut body = Dom::create_div().with_css(
        "display: flex; flex-direction: row; flex-grow: 1; min-height: 0px; min-width: 0px;",
    );
    body.add_child(pages);
    if s.voice.notes_open {
        body.add_child(column::column(s, data));
    }
    Dom::create_div()
        .with_css(COLUMN)
        .with_child(title_row(s, data))
        .with_child(page_rail(s, data))
        .with_child(body)
}

/// What went wrong (a session that was not written), over the pages until
/// dismissed.
fn problem_bar(problem: &str, data: &RefAny) -> Dom {
    InfoBar::create(AzString::from(problem))
        .with_kind(AlertKind::Danger)
        .with_action(label("azreview-dismiss"))
        .with_on_action(data.clone(), crate::on_dismiss_problem as ButtonOnClickCallbackType)
        .dom()
}

/// The window's top row; there is no title bar. The row moves the window
/// (`-azul-app-region: drag`; macOS's traffic lights sit at its left, in the
/// room `TabsInTitlebar` leaves them, and it is as tall as their bar) and
/// holds the files' switch, the five meanings (keys 1 to 5) and the nib.
/// The folder under review as the title row names it: the folder's own name, and the whole path (the chip's
/// tooltip). None for no folder.
fn folder_names(root: &std::path::Path) -> Option<(String, String)> {
    if root.as_os_str().is_empty() {
        return None;
    }
    let whole = root.display().to_string();
    let name = root
        .file_name()
        .map_or_else(|| whole.clone(), |n| n.to_string_lossy().into_owned());
    Some((name, whole))
}

fn title_row(s: &AppState, data: &RefAny) -> Dom {
    let chrome = kit::tabs_in_titlebar();
    let mut row = Dom::create_div().with_css(
        format!(
            "display: flex; flex-direction: row; align-items: center; gap: 3px; height: 28px; \
             box-sizing: border-box; padding: 0px {:.0}px 0px {:.0}px; flex-shrink: 0; \
             user-select: none; -azul-app-region: drag;",
            6.0 + chrome.right.max(0.0),
            6.0 + chrome.left.max(0.0),
        )
        .as_str(),
    );
    row.add_child(sidebar_switch(s, data));
    row.add_child(open_folder_button(data));
    if let Some(chip) = folder_chip(s) {
        row.add_child(chip);
    }
    row.add_child(Dom::create_div().with_css("width: 8px; flex-shrink: 0;"));
    for (i, sem) in Semantic::ALL.iter().enumerate() {
        row.add_child(meaning_key(i, *sem, *sem == s.ink_meaning(), data));
    }
    row.add_child(Dom::create_div().with_css("width: 8px; flex-shrink: 0;"));
    row.add_child(nib_key(s, data));
    row.add_child(Dom::create_div().with_css("width: 8px; flex-shrink: 0;"));
    row.add_child(column::switch(s, data));
    row
}

/// The title row's "Open folder...": a folder icon and the words, a key of its own (the folder dialog; the
/// session menu has it with its key, Mod+O).
fn open_folder_button(data: &RefAny) -> Dom {
    let name = label("azreview-open-folder");
    with_tip(
        key(
            "padding: 0px 8px; background: system:control-background; border: 1px solid system:separator; \
             color: system:text; :hover { background: system:selection-background; }",
            name.clone(),
            data.clone(),
            crate::on_open_folder,
        ),
        &name,
    )
    .with_id(ids::OPEN_FOLDER_BAR)
    .with_child(Dom::create_icon("folder_open").with_css("font-size: 14px;"))
    .with_child(Dom::create_span_with_text(name.as_str()))
}

/// Which folder is under review: its name, the file open after it; the whole path is the tooltip (the title
/// bar is the app's own, so the window's title shows nothing). None while there is no folder.
fn folder_chip(s: &AppState) -> Option<Dom> {
    let (name, whole) = folder_names(&s.root)?;
    let file = s.file().map(|f| f.display.clone());
    let tip = match &file {
        Some(f) => format!("{whole}/{f}"),
        None => whole,
    };
    let mut chip = Dom::create_div()
        .with_id(ids::FOLDER_CHIP)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; gap: 6px; min-width: 0px; flex-shrink: 1; \
             overflow: hidden; white-space: nowrap; font-size: 11px; padding: 0px 4px;",
        )
        .with_child(
            Dom::create_span_with_text(name.as_str())
                .with_css("font-weight: 600; color: system:text; flex-shrink: 0; max-width: 220px;"),
        );
    if let Some(file) = file {
        chip.add_child(
            Dom::create_span_with_text(file.as_str())
                .with_css("color: system:secondary-text; min-width: 0px; overflow: hidden;"),
        );
    }
    Some(with_tip(chip, &AzString::from(tip.as_str())))
}

/// The files' switch: flies the files out over the pages, folds them away.
fn sidebar_switch(s: &AppState, data: &RefAny) -> Dom {
    let name = label("azreview-files-switch");
    let face = if s.sidebar_open {
        "padding: 0px 3px; background: system:control-background; border: 1px solid \
         system:separator; color: system:text;"
    } else {
        QUIET
    };
    with_tip(key(face, name.clone(), data.clone(), crate::on_toggle_sidebar), &name)
        .with_id(ids::SIDEBAR_TOGGLE)
        .with_child(Dom::create_icon("view_sidebar").with_css("font-size: 15px;"))
}

/// Meaning `sem` (key `i + 1`): its icon and digit on a tint of its colour,
/// filled while it is the meaning the pen writes.
fn meaning_key(i: usize, sem: Semantic, active: bool, data: &RefAny) -> Dom {
    let c = sem.color();
    let face = if active {
        format!(
            "background: rgba({0},{1},{2},1.0); color: #ffffff; border: 1px solid \
             rgba({0},{1},{2},1.0);",
            c.r, c.g, c.b
        )
    } else {
        format!(
            "background: rgba({0},{1},{2},0.16); color: system:text; border: 1px solid \
             rgba({0},{1},{2},0.35);",
            c.r, c.g, c.b
        )
    };
    let name = AzString::from(crate::l10n::word("meaning", sem.label()));
    with_tip(key(&face, name.clone(), data.clone(), crate::on_pick_semantic), &name)
        .with_dataset(OptionRefAny::Some(RefAny::new(IndexTag { index: i })))
        .with_child(Dom::create_icon(sem.icon()).with_css("font-size: 13px;"))
        .with_child(Dom::create_span_with_text(format!("{}", i + 1).as_str()).with_css("font-size: 10px;"))
}

/// The nib the pen draws with (marker, pen, audio pen); a click takes the
/// next one.
fn nib_key(s: &AppState, data: &RefAny) -> Dom {
    let nib = crate::l10n::word("nib", s.tool.label());
    let name = AzString::from(format!("{}: {nib}", t("azreview-cycle-nib")));
    key(
        "background: transparent; border: 1px dashed system:separator; color: system:text; \
         :hover { background: system:control-background; }",
        name,
        data.clone(),
        crate::on_cycle_tool,
    )
    .with_child(Dom::create_icon(s.tool.icon()).with_css("font-size: 14px;"))
    .with_child(Dom::create_span_with_text(nib.as_str()))
}

/// The files, flying out over the pages from the left like macOS's sidebar
/// in a narrow window: out while the switch has them out, folded away when a
/// file is picked or with Escape.
fn flyout(s: &AppState, data: &RefAny) -> Dom {
    let mut list = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: \
             auto; overflow-x: hidden; padding: 4px 0px;",
        )
        .with_id(ids::FILES)
        .with_accessibility_name(label("azreview-files"));

    let mut prev: Vec<String> = Vec::new();
    for (i, f) in s.files.iter().enumerate() {
        let parts: Vec<&str> = f.display.split(['/', '\\']).collect();
        let (dirs, name) = parts.split_at(parts.len().saturating_sub(1));
        for (depth, comp) in dirs.iter().enumerate() {
            if prev.get(depth).map(String::as_str) == Some(*comp) {
                continue;
            }
            list.add_child(finder_row(
                comp, depth, "folder", "system:accent", false, None, data,
            ));
        }
        prev = dirs.iter().map(|c| (*c).to_string()).collect();
        let leaf = name.first().copied().unwrap_or(f.display.as_str());
        list.add_child(finder_row(
            leaf,
            dirs.len(),
            file_icon(leaf),
            "system:secondary-text",
            s.current == Some(i),
            Some(i),
            data,
        ));
    }
    Dom::create_div()
        .with_id(ids::FLYOUT)
        .with_css(
            "position: absolute; top: 6px; left: 6px; bottom: 6px; width: 260px; display: flex; \
             flex-direction: column; background: system:window-background; border: 1px solid \
             system:separator; border-radius: 8px; box-shadow: 0px 6px 20px rgba(0,0,0,0.28); \
             overflow: hidden;",
        )
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; padding: 4px 6px 4px \
                     10px; flex-shrink: 0; border-bottom: 1px solid system:separator;",
                )
                .with_child(
                    Dom::create_span_with_text(label("azreview-files"))
                        .with_css("font-size: 11px; color: system:secondary-text; flex-grow: 1;"),
                )
                .with_child(open_folder_key(data)),
        )
        .with_child(list)
}

/// The flyout's "Open folder": a folder dialog, the folder's files in the flyout.
fn open_folder_key(data: &RefAny) -> Dom {
    let name = label("azreview-open-folder");
    with_tip(key(QUIET, name.clone(), data.clone(), crate::on_open_folder), &name)
        .with_id(ids::OPEN_FOLDER)
        .with_child(Dom::create_icon("folder_open").with_css("font-size: 15px;"))
}

/// The primary modifier of a menu item's key: Command on a Mac, Control elsewhere.
const fn cmd_key() -> VirtualKeyCode {
    if cfg!(target_os = "macos") {
        VirtualKeyCode::LWin
    } else {
        VirtualKeyCode::LControl
    }
}

fn menu_bar(data: &RefAny) -> Menu {
    let mut items: Vec<MenuItem> = Vec::new();

    let mut ink = StringMenuItem::create(label("azreview-menu-ink"));
    for (i, sem) in Semantic::ALL.iter().enumerate() {
        ink = ink.with_child(MenuItem::String(
            StringMenuItem::create(crate::l10n::word("meaning", sem.label()))
                .with_callback(RefAny::new(IndexTag { index: i }), crate::on_menu_semantic),
        ));
    }
    ink = ink.with_child(MenuItem::Separator);
    ink = ink.with_child(MenuItem::String(
        StringMenuItem::create(label("azreview-undo")).with_callback(data.clone(), crate::on_menu_undo),
    ));
    ink = ink.with_child(MenuItem::String(
        StringMenuItem::create(label("azreview-redo")).with_callback(data.clone(), crate::on_menu_redo),
    ));
    ink = ink.with_child(MenuItem::Separator);
    ink = ink.with_child(MenuItem::String(
        StringMenuItem::create(label("azreview-cycle-nib")).with_callback(data.clone(), crate::on_cycle_tool),
    ));
    ink = ink.with_child(MenuItem::String(
        StringMenuItem::create(label("azreview-toggle-recording"))
            .with_callback(data.clone(), crate::on_toggle_record),
    ));
    items.push(MenuItem::String(ink));

    let mut session = StringMenuItem::create(label("azreview-menu-session"));
    let mut open = StringMenuItem::create(label("azreview-open-folder"))
        .with_callback(data.clone(), crate::on_open_folder);
    open.accelerator = OptionVirtualKeyCodeCombo::Some(VirtualKeyCodeCombo {
        keys: vec![cmd_key(), VirtualKeyCode::O].into(),
    });
    session = session.with_child(MenuItem::String(open));
    session = session.with_child(MenuItem::String(
        StringMenuItem::create(label("azreview-save-now")).with_callback(data.clone(), crate::on_menu_save),
    ));
    session = session.with_child(MenuItem::String(
        StringMenuItem::create(label("azreview-reveal-archive"))
            .with_callback(data.clone(), crate::on_menu_reveal),
    ));
    session = session.with_child(MenuItem::String(
        StringMenuItem::create(label("azreview-save-pointer-log"))
            .with_callback(data.clone(), crate::on_menu_pointer_log),
    ));
    items.push(MenuItem::String(session));

    Menu::create(MenuItemVec::copy_from_ptr(&items[0], items.len()))
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
    // A selected file: flat's accent row; under flora flora's selection (the soft wash and
    // the deep ink by day, the stone at night), as every list of the engine shows it. A file
    // is a button (a folder only a heading), without the UA's border and font.
    let mut row = match index {
        Some(_) => Dom::create_node(NodeType::Button),
        None => Dom::create_div(),
    }
    .with_css(
        format!(
            "display: flex; flex-direction: row; align-items: center; gap: 6px; padding: 3px 10px \
             3px {}px; font-size: 11px; flex-shrink: 0; border: 0px solid transparent; \
             background: {}; color: {};{}",
            10 + depth * 16,
            if selected { "system:accent" } else { "transparent" },
            if selected { "system:accent-text" } else { "system:text" },
            if selected {
                " @theme(flora) { background: system:selection-background; color: \
                 system:selection-text; }"
            } else {
                ""
            },
        )
        .as_str(),
    );
    row.add_child(
        Dom::create_icon(icon).with_css(
            format!(
                "font-size: 14px; color: {};{}",
                if selected { "system:accent-text" } else { icon_color },
                // Under flora a glyph is flora's small-icon ink (a multicolour Haiku icon keeps
                // its own colours whatever the ink).
                if selected {
                    " @theme(flora) { color: system:selection-text; }"
                } else {
                    " @theme(flora) { color: system:icon; }"
                },
            )
            .as_str(),
        ),
    );
    row.add_child(Dom::create_span_with_text(label));
    match index {
        Some(i) => row
            .with_dataset(OptionRefAny::Some(RefAny::new(IndexTag { index: i })))
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                data.clone(),
                crate::on_pick_file,
            ),
        None => row,
    }
}

/// The recording's meter: how long the clip is and the level of the last packet of voice, a level
/// meter that tells a screen reader its name.
fn meter(s: &AppState) -> Dom {
    let seconds = s.level_samples / crate::CLIP_RATE as usize;
    let mut wrap = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; gap: 6px;");
    wrap.add_child(
        Dom::create_span_with_text(format!("{}:{:02}", seconds / 60, seconds % 60).as_str())
            .with_css("font-size: 11px; color: system:secondary-text; font-family: monospace;"),
    );
    let mut holder = Dom::create_div().with_css("width: 160px;");
    holder.add_child(
        LevelMeter::create(s.level)
            .with_accessibility_name(label("azreview-recording-level"))
            .dom(),
    );
    wrap.add_child(holder);
    wrap
}

/// The microphone, a capture widget without a picture (1 px): mounted while a clip records.
fn microphone(data: &RefAny) -> Dom {
    MicrophoneWidget::create(AudioConfig {
        sample_rate: crate::CLIP_RATE,
        channels: 1,
    })
    .with_on_frame(data.clone(), crate::on_audio_frame)
    .dom()
    .with_id(ids::MICROPHONE)
    .with_css("width: 1px; height: 1px; overflow: hidden; flex-shrink: 0;")
}

pub const STRIP_ID: &str = ids::STRIP_NAME;

/// The page numbers - the one in view lit, a click scrolls the strip to its
/// page - and the tools: the voice meter while recording, record, save and
/// the settings, all one height.
fn page_rail(s: &AppState, data: &RefAny) -> Dom {
    let mut rail = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 4px; padding: 3px 8px; \
         border-bottom: 1px solid system:separator; flex-shrink: 0; user-select: none;",
    );
    let mut pages = Dom::create_div().with_id(ids::PAGES).with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 3px; flex-grow: 1; \
         flex-shrink: 1; flex-basis: 0px; min-width: 0px; overflow-x: auto; overflow-y: hidden;",
    );
    if let Some(file) = s.file() {
        for page in 0..file.page_count() {
            pages.add_child(page_pill(page, page == s.visible_page, data));
        }
    }
    rail.add_child(pages);

    let rec = s.recording.is_some();
    if rec {
        rail.add_child(meter(s));
        // The capture runs while it is mounted: its frames are the clip's samples.
        rail.add_child(microphone(data));
    }
    let (icon, name) = if rec {
        ("fiber_manual_record", "azreview-stop-recording")
    } else {
        ("mic", "azreview-record")
    };
    rail.add_child(tool(icon, label(name), rec, data, crate::on_toggle_record).with_id(ids::RECORD));
    rail.add_child(
        tool("save", label("azreview-save"), false, data, crate::on_save_button).with_id(ids::SAVE),
    );
    rail.add_child(
        tool("settings", label("azreview-settings"), false, data, crate::on_settings_open)
            .with_id(ids::SETTINGS),
    );
    rail
}

/// Page `page`'s number: as wide as the number and its padding, lit while
/// that page is in view.
fn page_pill(page: usize, here: bool, data: &RefAny) -> Dom {
    // Flat's monospace; under flora flora's Garamond (its figures are tabular) at 12px.
    let face = if here {
        "background: system:accent; color: system:accent-text; border: 1px solid system:accent;"
    } else {
        "background: system:control-background; color: system:secondary-text; border: 1px solid \
         system:separator;"
    };
    let name = AzString::from(t_args("azreview-page", &[("n", Arg::from(page + 1))]));
    key(
        &format!(
            "padding: 0px 7px; font-family: monospace; {face} @theme(flora) {{ font-family: EB \
             Garamond, Georgia, serif; font-size: 12px; }}"
        ),
        name,
        data.clone(),
        crate::on_jump_to_page,
    )
    .with_dataset(OptionRefAny::Some(RefAny::new(IndexTag { index: page })))
    .with_child(Dom::create_span_with_text(format!("{}", page + 1).as_str()))
}

/// An icon-only tool of the rail named `name` (the screen reader's word and
/// the tooltip); `on` lights it (recording).
fn tool(icon: &str, name: AzString, on: bool, data: &RefAny, on_click: CallbackType) -> Dom {
    let face = if on {
        "padding: 0px 3px; background: #d62d20; border: 1px solid #d62d20; color: #ffffff;"
    } else {
        QUIET
    };
    with_tip(key(face, name.clone(), data.clone(), on_click), &name)
        .with_child(Dom::create_icon(icon).with_css("font-size: 15px;"))
}

/// The page in view at the strip's scroll `x` (of `pages`): the one whose
/// sheet starts nearest the strip's left edge.
#[must_use]
pub fn page_at(x: f32, pages: usize) -> usize {
    ((x.max(0.0) / page_stride()).round() as usize).min(pages.saturating_sub(1))
}

/// How far the strip is scrolled sideways (`None`: no strip).
pub fn strip_scroll_x(info: &CallbackInfo) -> Option<f32> {
    let dom = info.get_hit_node().dom;
    let node = info.get_node_id_by_id_attribute(dom, STRIP_ID);
    // 0 is "no node"; a node's raw id is its index + 1.
    let index = node.into_raw().checked_sub(1)?;
    info.get_scroll_offset_for_node(dom, NodeId::create(index))
        .into_option()
        .map(|offset| offset.x)
}

/// Scrolls the page numbers' row so page `page`'s number is in view. Its
/// place in the row is the widths of the numbers before it and their gaps.
pub fn reveal_pill(info: &mut CallbackInfo, page: usize) {
    const GAP: f32 = 3.0;
    let dom = info.get_hit_node().dom;
    let row = info.get_node_id_by_id_attribute(dom, ids::PAGES_NAME);
    let Some(index) = row.into_raw().checked_sub(1) else {
        return;
    };
    let row_node = DomNodeId { dom, node: row };
    let Some(view) = info.get_node_rect(row_node).into_option() else {
        return;
    };
    let mut left = 0.0;
    let mut pill = info.get_first_child(row_node).into_option();
    for _ in 0..page {
        let Some(p) = pill else {
            return;
        };
        left += info.get_node_rect(p).into_option().map_or(0.0, |r| r.size.width) + GAP;
        pill = info.get_next_sibling(p).into_option();
    }
    let Some(width) = pill.and_then(|p| info.get_node_rect(p).into_option()).map(|r| r.size.width)
    else {
        return;
    };
    let scrolled = info
        .get_scroll_offset_for_node(dom, NodeId::create(index))
        .into_option()
        .map_or(0.0, |offset| offset.x);
    let target = if left < scrolled {
        left
    } else if left + width > scrolled + view.size.width {
        left + width - view.size.width
    } else {
        return;
    };
    info.scroll_to(dom, row, LogicalPosition::create(target.max(0.0), 0.0));
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
        "flex-grow: 1; min-height: 0px; background: system:under-page-background; display: flex; \
         flex-direction: column; overflow: hidden; padding: 18px;",
    );
    if s.file().is_none() {
        // No file open: no folder yet (the app started with none to review), none to review under it, or none
        // picked. The way out is in the page itself, large.
        let (says, explains) = if s.root.as_os_str().is_empty() {
            (
                AzString::from(t("azreview-no-folder")),
                Some(AzString::from(t("azreview-no-folder-hint"))),
            )
        } else if s.files.is_empty() {
            (
                AzString::from(t_args(
                    "azreview-no-files",
                    &[("folder", Arg::from(s.root.display().to_string()))],
                )),
                None,
            )
        } else {
            (label("azreview-open-file"), None)
        };
        let mut panel = Dom::create_div()
            .with_css(
                "flex-grow: 1; display: flex; flex-direction: column; align-items: center; justify-content: \
                 center; gap: 14px; padding: 40px; text-align: center;",
            )
            .with_child(Dom::create_icon("folder_open").with_css("font-size: 56px; color: system:secondary-text;"))
            .with_child(
                Dom::create_p_with_text(says)
                    .with_css("margin: 0px; font-size: 18px; font-weight: 600; color: system:text;"),
            );
        if let Some(explains) = explains {
            panel.add_child(
                Dom::create_p_with_text(explains)
                    .with_css("margin: 0px; font-size: 13px; color: system:secondary-text; max-width: 420px;"),
            );
        }
        let name = label("azreview-open-folder");
        panel.add_child(
            Dom::create_node(NodeType::Button)
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; gap: 10px; height: 44px; padding: \
                     0px 22px; font-size: 15px; border-radius: 8px; border: 0px solid transparent; background: \
                     system:accent; color: system:accent-text; cursor: pointer; margin-top: 6px;",
                )
                .with_accessibility_name(name.clone())
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    data.clone(),
                    crate::on_open_folder,
                )
                .with_id(ids::OPEN_FOLDER_EMPTY)
                .with_child(Dom::create_icon("folder_open").with_css("font-size: 20px;"))
                .with_child(Dom::create_span_with_text(name.as_str())),
        );
        area.add_child(panel);
        return area;
    }
    area.add_child(
        Dom::create_virtual_view(
            RefAny::new(SheetStrip { app: data.clone() }),
            sheets_virtual_view,
        )
        .with_id(ids::STRIP)
        .with_accessibility_info(AccessibilityInfo::named(
            label("azreview-pages"),
            AccessibilityRole::Document,
        ))
        .with_css("flex-grow: 1; min-height: 0px; width: 100%;")
        // A wheel or a trackpad scrolling the sheets lights the number of the
        // page that comes into view.
        .with_callback(EventFilter::Hover(HoverEventFilter::Scroll), data.clone(), crate::on_strip_scroll)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::ScrollEnd),
            data.clone(),
            crate::on_strip_scroll,
        ),
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
        let leftmost = page_at(info.scroll_offset.x, total);
        let first_visible = (info.scroll_offset.x.max(0.0) / stride) as usize;
        let first = first_visible.saturating_sub(1);
        let visible = (viewport_w / stride).ceil() as usize + 2;
        let count = visible.max(2).min(total.saturating_sub(first));

        let mut row = Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: flex-start;");
        let listening = s.voice.running_note().filter(|_| s.voice.is_listening());
        for page in first..first + count {
            row.add_child(page_sheet(&cb_app, file, page, &s.notes, listening));
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

fn page_sheet(
    data: &RefAny,
    file: &code::SourceFile,
    page: usize,
    notes: &[Note],
    listening: Option<u64>,
) -> Dom {
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

    // The marker's ink first: positioned boxes paint in tree order, so the
    // code (positioned too) is drawn over it, and the pen's ink over both.
    sheet.add_child(ink_layer(data, page, Layer::Marker));
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
            "position: relative; display: flex; flex-direction: column; width: {}px; overflow: \
             hidden; user-select: none;",
            (PAGE_W - MARGIN_GUTTER_W - PAGE_PAD) as isize,
        )
        .as_str(),
    );
    for (i, line) in lines.iter().enumerate() {
        col.add_child(code_row(first_line + i, line.as_str()));
    }
    sheet.add_child(col);
    // The markers of the dictated notes: over the code, under the pen layer (they take no stroke).
    for marker in column::markers(notes, page, listening) {
        sheet.add_child(marker);
    }
    sheet.add_child(ink_layer(data, page, Layer::Pen));
    sheet
}

/// One line of code: its number in the gutter, then the line.
fn code_row(number: usize, line: &str) -> Dom {
    let mut row = Dom::create_div()
        .with_css(format!("display: flex; flex-direction: row; height: {LINE_H}px;").as_str());
    row.add_child(
        Dom::create_span_with_text(format!("{number}").as_str()).with_css(
            format!(
                "width: {}px; flex-shrink: 0; text-align: right; padding-right: 10px; \
                 font-family: monospace; font-size: 11px; color: #b0aaa0;",
                GUTTER_W as isize - 10,
            )
            .as_str(),
        ),
    );
    row.add_child(Dom::create_span_with_text(line).with_css(
        "font-family: monospace; font-size: 11px; color: #1f1f1f; white-space: pre;",
    ));
    row
}

/// What the screen reader is told of a picture that only decorates: nothing.
fn decorative() -> AccessibilityInfo {
    AccessibilityInfo {
        role: AccessibilityRole::Nothing,
        ..AccessibilityInfo::create_default()
    }
}

/// The `layer` of page `page`'s ink: an [`InkCanvas`] drawn by
/// [`ink::render_ink`], its frame kept over a DOM rebuild. The pen layer, on
/// top, takes the pointer for the app (`data`).
fn ink_layer(data: &RefAny, page: usize, layer: Layer) -> Dom {
    let canvas = RefAny::new(InkCanvas::create(data.clone(), page, layer));
    let dom = Dom::create_image(ImageRef::callback(
        RenderImageCallback::create(ink::render_ink).to_core(),
        canvas.clone(),
    ))
    .with_dataset(OptionRefAny::Some(canvas))
    .with_merge_callback(ink::merge_ink)
    .with_css(
        format!(
            "position: absolute; top: 0px; left: 0px; width: {}px; height: {}px;",
            PAGE_W as isize,
            page_h() as isize,
        )
        .as_str(),
    );
    // The marker's layer lies under the code and takes nothing: it only decorates the page. The pen's is
    // the page's drawing surface, which a screen reader is told by name.
    if layer == Layer::Marker {
        return dom.with_accessibility_info(decorative());
    }
    dom.with_accessibility_info(AccessibilityInfo::named(
        t_args("azreview-ink-surface", &[("n", Arg::from(page + 1))]),
        AccessibilityRole::Graphic,
    ))
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseDown),
        data.clone(),
        crate::on_ink_down,
    )
    // Movement: `MouseMove` (W3C `mouseover` fires once, on entry - with it
    // a stroke kept only its first point).
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseMove),
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
    )
    // The pointer off the page, or the pen out of the tablet's range: no rest to time, and a stroke whose
    // release was not heard is over.
    .with_callback(
        EventFilter::Hover(HoverEventFilter::MouseLeave),
        data.clone(),
        crate::on_ink_leave,
    )
    .with_callback(
        EventFilter::Hover(HoverEventFilter::PenLeave),
        data.clone(),
        crate::on_ink_leave,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The events the ink layer hands to `handler`.
    fn events_of(layer: &Dom, handler: azul::callbacks::CallbackType) -> Vec<EventFilter> {
        layer
            .root
            .callbacks
            .as_slice()
            .iter()
            .filter(|c| c.callback.cb == handler as usize)
            .map(|c| c.event)
            .collect()
    }

    /// A line of code and its number are text spans, not anonymous divs: a
    /// screen reader is told what they are (and azul's div-as-text check stays
    /// quiet).
    #[test]
    fn a_code_line_and_its_number_are_text_spans() {
        let row = code_row(7, "fn main() {}");
        let kinds: Vec<bool> = row
            .children
            .as_slice()
            .iter()
            .map(|c| matches!(c.root.node_type, NodeType::Span))
            .collect();
        assert_eq!(kinds, vec![true, true]);
    }

    /// W3C `mouseover` fires once, when the pointer ENTERS the page; the
    /// points of a stroke between press and release come from `mousemove`.
    #[test]
    fn the_ink_layer_takes_the_points_of_a_stroke_from_every_pointer_move() {
        let layer = ink_layer(&RefAny::new(0u8), 0, Layer::Pen);
        assert_eq!(
            events_of(&layer, crate::on_ink_move),
            vec![EventFilter::Hover(HoverEventFilter::MouseMove)]
        );
        assert_eq!(
            events_of(&layer, crate::on_ink_down),
            vec![EventFilter::Hover(HoverEventFilter::MouseDown)]
        );
        assert_eq!(
            events_of(&layer, crate::on_ink_up),
            vec![EventFilter::Hover(HoverEventFilter::MouseUp)]
        );
    }

    /// AzReview with `files` files of three pages each, the first open.
    fn app(files: usize) -> AppState {
        crate::l10n::in_english();
        let lines = (0..code::LINES_PER_PAGE * 3)
            .map(|i| format!("fn step_{i}() {{}}"))
            .collect::<Vec<_>>();
        let files = (0..files)
            .map(|i| code::SourceFile {
                path: std::path::PathBuf::from(format!("f{i}.rs")),
                display: format!("f{i}.rs"),
                lines: lines.clone(),
            })
            .collect();
        AppState::create(
            std::path::PathBuf::new(),
            files,
            RefAny::new(0u8),
            std::path::PathBuf::new(),
        )
    }

    /// `dom` and every box inside it, in tree order.
    fn nodes(dom: &Dom) -> Vec<&Dom> {
        let mut out = vec![dom];
        for child in dom.children.as_slice() {
            out.extend(nodes(child));
        }
        out
    }

    fn with_id<'a>(dom: &'a Dom, id: &azul::str::String) -> Option<&'a Dom> {
        nodes(dom).into_iter().find(|n| n.root.has_id(id.clone()))
    }

    /// The words inside `dom` (an icon's glyph leaf is empty: no word).
    fn texts(dom: &Dom) -> Vec<String> {
        nodes(dom)
            .into_iter()
            .filter_map(|n| n.root.node_type.get_text().into_option())
            .map(|t| t.as_str().to_string())
            .filter(|t| !t.trim().is_empty())
            .collect()
    }

    /// Whether `dom`'s own CSS (the app's `with_css`) declares `what`, as the
    /// property prints (`AppRegion(Exact(Drag))`). Through the library a
    /// declaration prints over several lines (`{:#?}`), so the print is read
    /// on one line first.
    fn declares(dom: &Dom, what: &str) -> bool {
        dom.css
            .as_slice()
            .iter()
            .flat_map(|css| css.rules.as_slice().iter())
            .flat_map(|rule| rule.declarations.as_slice().iter())
            .any(|d| one_line(&format!("{d:?}")).contains(what))
    }

    /// A Debug print on one line: no whitespace, no comma before a closing
    /// bracket (`Exact(\n    Drag,\n)` reads `Exact(Drag)`).
    fn one_line(printed: &str) -> String {
        let flat: String = printed.chars().filter(|c| !c.is_whitespace()).collect();
        flat.replace(",)", ")").replace(",}", "}").replace(",]", "]")
    }

    fn buttons(dom: &Dom) -> Vec<&Dom> {
        nodes(dom)
            .into_iter()
            .filter(|n| matches!(n.root.node_type, NodeType::Button))
            .collect()
    }

    /// No title bar: the title row moves the window (macOS's traffic lights
    /// in it), and what sits in it stays pressable.
    #[test]
    fn the_title_row_moves_the_window_and_its_controls_stay_controls() {
        let s = app(2);
        let row = title_row(&s, &RefAny::new(0u8));
        assert!(declares(&row, "AppRegion(Exact(Drag))"), "the row moves the window");
        let controls = buttons(&row);
        assert!(!controls.is_empty());
        for control in controls {
            assert!(
                declares(control, "AppRegion(Exact(NoDrag))"),
                "a control in the row is pressed, not dragged: {:?}",
                texts(control)
            );
        }
    }

    #[test]
    fn the_title_row_holds_the_sidebar_switch_and_the_five_meanings() {
        let s = app(2);
        let row = title_row(&s, &RefAny::new(0u8));
        assert!(with_id(&row, &ids::SIDEBAR_TOGGLE).is_some(), "the files' switch");
        let shown = texts(&row);
        for digit in ["1", "2", "3", "4", "5"] {
            assert!(shown.iter().any(|t| t == digit), "meaning {digit} in {shown:?}");
        }
        assert!(!shown.iter().any(|t| t == "AzReview"), "no title: {shown:?}");
    }

    #[test]
    fn the_window_has_no_status_bar() {
        let s = app(2);
        let view = main_view(&s, &RefAny::new(0u8));
        assert!(nodes(&view)
            .iter()
            .all(|n| !n.root.has_class(azul::str::String::from("__azul-native-statusbar"))));
    }

    #[test]
    fn the_files_fly_out_over_the_pages_and_fold_away() {
        let mut s = app(2);
        s.sidebar_open = true;
        let view = main_view(&s, &RefAny::new(0u8));
        let flyout = with_id(&view, &ids::FLYOUT).expect("the files fly out");
        assert!(declares(flyout, "Position(Exact(Absolute))"), "over the pages, not beside them");
        assert!(texts(flyout).iter().any(|t| t == "f1.rs"), "it lists the files");

        s.sidebar_open = false;
        let view = main_view(&s, &RefAny::new(0u8));
        assert!(with_id(&view, &ids::FLYOUT).is_none(), "folded away");
    }

    #[test]
    fn picking_a_file_opens_it_and_folds_the_files_away() {
        let mut s = app(2);
        s.sidebar_open = true;
        s.open_file(1);
        assert_eq!(s.current, Some(1));
        assert!(!s.sidebar_open, "the flyout gets out of the way of the pages");
    }

    /// An icon is enough for record, save and settings; the name is for the
    /// screen reader and the tooltip.
    #[test]
    fn record_save_and_settings_are_icon_only_buttons() {
        let s = app(1);
        let rail = page_rail(&s, &RefAny::new(0u8));
        for id in [&ids::RECORD, &ids::SAVE, &ids::SETTINGS] {
            let button = with_id(&rail, id).unwrap_or_else(|| panic!("{} in the rail", id.as_str()));
            assert!(matches!(button.root.node_type, NodeType::Button), "{}", id.as_str());
            assert!(texts(button).is_empty(), "{} shows no words: {:?}", id.as_str(), texts(button));
        }
    }

    /// A click on a control focuses the control; nothing around the pages is
    /// a keyboard stop a click could fall back to (the shell's panes were,
    /// and a click on the pages rang the document pane).
    #[test]
    fn every_control_of_the_window_is_a_button() {
        let mut s = app(2);
        s.sidebar_open = true;
        let view = main_view(&s, &RefAny::new(0u8));
        for n in nodes(&view) {
            // What answers a press (the strip only listens to its scroll).
            let pressed = n.root.callbacks.as_slice().iter().any(|c| {
                matches!(
                    c.event,
                    EventFilter::Hover(
                        HoverEventFilter::Click | HoverEventFilter::MouseUp | HoverEventFilter::MouseDown
                    )
                )
            });
            if !pressed {
                continue;
            }
            assert!(
                matches!(n.root.node_type, NodeType::Button),
                "{:?} answers the pointer but is no button: {:?}",
                n.root.node_type,
                texts(n)
            );
        }
        assert!(nodes(&view)
            .iter()
            .all(|n| !matches!(n.root.node_type, NodeType::Main | NodeType::Nav)));
    }

    #[test]
    fn a_failed_save_is_told_over_the_pages_until_dismissed() {
        let mut s = app(1);
        s.problem = Some("the disk is full".to_string());
        let view = main_view(&s, &RefAny::new(0u8));
        assert!(texts(&view).iter().any(|t| t.contains("the disk is full")));

        s.problem = None;
        let view = main_view(&s, &RefAny::new(0u8));
        assert!(!texts(&view).iter().any(|t| t.contains("the disk is full")));
    }

    /// The page in view is the one whose sheet starts nearest the strip's
    /// left edge - what the lit number shows.
    #[test]
    fn the_page_in_view_is_the_sheet_nearest_the_strips_left_edge() {
        let stride = page_stride();
        assert_eq!(page_at(0.0, 3), 0);
        assert_eq!(page_at(stride * 1.4, 3), 1);
        assert_eq!(page_at(stride * 1.6, 3), 2);
        assert_eq!(page_at(stride * 9.0, 3), 2, "past the last sheet: the last");
        assert_eq!(page_at(-5.0, 3), 0, "an overscroll before the first: the first");
    }

    /// A wheel or a trackpad scrolling the strip tells the app, which lights
    /// the number of the page that came into view (it went stale before).
    #[test]
    fn the_strip_tells_the_app_when_it_scrolls() {
        let s = app(1);
        let area = sheet(&s, &RefAny::new(0u8));
        let strip = with_id(&area, &ids::STRIP).expect("the sheet strip");
        let events = events_of(strip, crate::on_strip_scroll);
        assert!(events.contains(&EventFilter::Hover(HoverEventFilter::Scroll)), "{events:?}");
        assert!(events.contains(&EventFilter::Hover(HoverEventFilter::ScrollEnd)), "{events:?}");
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
