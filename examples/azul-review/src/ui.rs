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
    widgets::{AlertKind, InfoBar, LevelMeter, MicrophoneWidget, Spinner, SpinnerStyle},
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

/// The measures of a page, one place for them: the whiteboard's cutouts are laid out by the same numbers, so a
/// cutout is as wide and as high per line as the page it was taken from.
pub const LINE_H: f32 = 15.0;
pub(crate) const PAGE_W: f32 = 1000.0;
pub(crate) const GUTTER_W: f32 = 52.0;

pub(crate) const MARGIN_GUTTER_W: f32 = 260.0;
pub const PAGE_PAD: f32 = 32.0;
/// How wide the code column of a page is (the page, less the notes' margin and the padding).
pub(crate) const CODE_W: f32 = PAGE_W - MARGIN_GUTTER_W - PAGE_PAD;
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
        main_view(&s, &data, info.get_window_width())
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

/// The width of the tree's switch, which sits next to the traffic lights.
const SWITCH_W: f32 = 28.0;

/// The review window: the file tree (the whole height of the window, it pushes the rest to the right), and beside it a
/// column of the top bar, the middle (the page strip, the pages, the right bar and its panel) and the bottom bar.
/// `window_w` is the window's width now (logical px): below `layout::OVERLAY_BELOW_W` the tree covers the pages.
pub fn main_view(s: &AppState, data: &RefAny, window_w: f32) -> Dom {
    let chrome = kit::tabs_in_titlebar();
    let reserved = chrome.left.max(0.0) + SWITCH_W + 12.0;
    let solved = crate::layout::solve(window_w, &s.panels, &s.slide, reserved);
    let has_files = !s.files.is_empty();
    let mut root = Dom::create_div().with_css(
        "position: relative; display: flex; flex-direction: row; flex-grow: 1; min-height: 0px; min-width: 0px;",
    );
    if has_files && solved.tree_w > 0.0 {
        root.add_child(tree_column(s, data, &solved));
    }
    root.add_child(main_column(s, data, &solved));
    if has_files {
        root.add_child(tree_switch(s, data, chrome.left.max(0.0) + 6.0));
    }
    root
}

/// The file tree: as tall as the window, `solved.tree_w` wide while it slides, its rows laid out at the width it
/// slides out to; no heading. Where there is no room to push the pages it covers them.
fn tree_column(s: &AppState, data: &RefAny, solved: &crate::layout::Solved) -> Dom {
    let placement = if solved.overlay {
        "position: absolute; left: 0px; top: 0px; bottom: 0px; z-index: 5; box-shadow: 0px 0px 24px rgba(0,0,0,0.35);"
    } else {
        "position: relative; flex-shrink: 0;"
    };
    let inner = Dom::create_div()
        .with_css(
            format!(
                "display: flex; flex-direction: column; flex-grow: 1; flex-shrink: 0; min-height: 0px; width: {:.0}px;",
                solved.tree_full_w
            )
            .as_str(),
        )
        // The traffic lights and the switch are over this band; it moves the window.
        .with_child(
            Dom::create_div()
                .with_css(format!("height: {}px; flex-shrink: 0; -azul-app-region: drag;", crate::layout::TOP_H).as_str()),
        )
        .with_child(file_list(s, data));
    Dom::create_div()
        .with_id(ids::TREE)
        .with_css(
            format!(
                "display: flex; flex-direction: column; width: {:.0}px; overflow: hidden; background: \
                 system:window-background; border-right: 1px solid system:separator; {placement}",
                solved.tree_w
            )
            .as_str(),
        )
        .with_child(inner)
        .with_child(grip(crate::layout::Panel::Tree, "right: 0px;", data))
}

/// A panel's edge, to be dragged: a thin strip along the edge that takes the pointer while the button is down.
fn grip(panel: crate::layout::Panel, edge: &str, data: &RefAny) -> Dom {
    let (id, name) = match panel {
        crate::layout::Panel::Tree => (ids::TREE_GRIP, label("azreview-resize-tree")),
        crate::layout::Panel::Right => (ids::RIGHT_GRIP, label("azreview-resize-panel")),
    };
    Dom::create_div()
        .with_id(id)
        .with_css(
            format!(
                "position: absolute; top: 0px; bottom: 0px; {edge} width: {}px; cursor: col-resize; z-index: 6;",
                crate::layout::GRIP_W
            )
            .as_str(),
        )
        .with_accessibility_name(name)
        .with_dataset(OptionRefAny::Some(RefAny::new(GripTag { panel })))
        .with_callback(EventFilter::Hover(HoverEventFilter::LeftMouseDown), data.clone(), crate::on_grip_down)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseMove), data.clone(), crate::on_grip_move)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), data.clone(), crate::on_grip_up)
}

/// Which panel's edge a grip is.
#[derive(Debug, Clone, Copy)]
pub struct GripTag {
    pub panel: crate::layout::Panel,
}

pub fn grip_of(info: &mut CallbackInfo) -> Option<crate::layout::Panel> {
    info.get_dataset(info.get_hit_node())
        .into_option()
        .and_then(|mut d| d.downcast_ref::<GripTag>().map(|t| t.panel))
}

/// The column beside the tree: the top bar, the middle, the bottom bar.
fn main_column(s: &AppState, data: &RefAny, solved: &crate::layout::Solved) -> Dom {
    Dom::create_div()
        .with_css(COLUMN)
        .with_child(top_bar(s, data, solved.top_pad_left))
        .with_child(middle(s, data, solved))
        .with_child(bottom_bar(s, data))
}

/// The middle: the page strip at the left, the pages (what went wrong over them), the side column of dictated notes,
/// the right bar and, beside it, the right panel.
fn middle(s: &AppState, data: &RefAny, solved: &crate::layout::Solved) -> Dom {
    let mut pages = Dom::create_div().with_css(
        "position: relative; display: flex; flex-direction: column; flex-grow: 1; min-height: \
         0px; min-width: 0px;",
    );
    if let Some(problem) = s.problem.as_deref() {
        pages.add_child(problem_bar(problem, data));
    }
    pages.add_child(sheet(s, data));
    let mut row = Dom::create_div().with_css(
        "display: flex; flex-direction: row; flex-grow: 1; min-height: 0px; min-width: 0px;",
    );
    if s.file().is_some() {
        row.add_child(page_strip(s, data));
    }
    row.add_child(pages);
    if s.voice.notes_open {
        row.add_child(column::column(s, data));
    }
    row.add_child(right_bar(data));
    if solved.right_w > 0.0 {
        row.add_child(right_panel(solved.right_w, data));
    }
    row
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

/// The folder under review as the top bar names it: the folder's own name, and the whole path (the chip's
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

/// The window's top row; there is no title bar. The row moves the window (`-azul-app-region: drag`; macOS's traffic
/// lights sit at its left, in the room `TabsInTitlebar` leaves them, and it is as tall as their bar). At its left the
/// folder (the button and its name), in its middle the five meanings (keys 1 to 5) and the nib, centred on the
/// window; at its right the notes' switch, the recording, save and the settings. `left_pad` is what keeps it clear of
/// the traffic lights and the tree's switch while the tree is not wide enough to be under them.
fn top_bar(s: &AppState, data: &RefAny, left_pad: f32) -> Dom {
    let chrome = kit::tabs_in_titlebar();
    let mut row = Dom::create_div().with_css(
        format!(
            "display: flex; flex-direction: row; align-items: center; gap: 3px; height: {}px; \
             box-sizing: border-box; padding: 0px {:.0}px 0px {:.0}px; flex-shrink: 0; \
             user-select: none; -azul-app-region: drag;",
            crate::layout::TOP_H,
            6.0 + chrome.right.max(0.0),
            6.0 + left_pad,
        )
        .as_str(),
    );
    // The two ends take the same room (`flex: 1 1 0`), so what is between them is centred.
    let mut left = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 3px; flex: 1 1 0px; min-width: 0px; \
         justify-content: flex-start;",
    );
    left.add_child(open_folder_button(data));
    if let Some(chip) = folder_chip(s) {
        left.add_child(chip);
    }
    row.add_child(left);
    let mut centre = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 3px; flex-shrink: 0;",
    );
    for (i, spec) in crate::pens::PENS.iter().enumerate() {
        centre.add_child(pen_key(i, spec.semantic, i == s.pens.selected(), s.pens.nib_of(i), data));
    }
    row.add_child(centre);
    let mut right = Dom::create_div().with_css(
        "display: flex; flex-direction: row; align-items: center; gap: 4px; flex: 1 1 0px; min-width: 0px; \
         justify-content: flex-end;",
    );
    for tool in tools(s, data) {
        right.add_child(tool);
    }
    row.add_child(right);
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

/// The files' switch, next to the traffic lights: the tree slides out and pushes the pages, or slides in. It stays
/// where it is whether the tree is out or not.
fn tree_switch(s: &AppState, data: &RefAny, left: f32) -> Dom {
    let name = label("azreview-files-switch");
    let look = if s.panels.tree_open {
        "padding: 0px 3px; background: system:control-background; border: 1px solid \
         system:separator; color: system:text;"
    } else {
        QUIET
    };
    let face = format!("position: absolute; left: {left:.0}px; top: 3px; z-index: 10; {look}");
    with_tip(key(&face, name.clone(), data.clone(), crate::on_toggle_sidebar), &name)
        .with_id(ids::SIDEBAR_TOGGLE)
        .with_child(Dom::create_icon("view_sidebar").with_css("font-size: 15px;"))
}

/// Pen `i` (key `i + 1`): the icon of its meaning and its digit on a tint of its colour, filled while it is the pen in
/// hand, and then also the icon of its nib. A click takes the pen, a double click (or its key again) takes its next nib.
fn pen_key(i: usize, sem: Semantic, selected: bool, nib: Option<crate::model::Tool>, data: &RefAny) -> Dom {
    let c = sem.color();
    let face = if selected {
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
    let meaning = crate::l10n::word("meaning", sem.label());
    let name = match nib {
        Some(nib) => AzString::from(format!("{meaning}: {}", crate::l10n::word("nib", nib.label()))),
        None => AzString::from(meaning),
    };
    let mut dom = with_tip(key(&face, name.clone(), data.clone(), crate::on_pick_semantic), &name)
        .with_dataset(OptionRefAny::Some(RefAny::new(IndexTag { index: i })))
        .with_callback(EventFilter::Hover(HoverEventFilter::DoubleClick), data.clone(), crate::on_pen_double_click)
        .with_child(Dom::create_icon(sem.icon()).with_css("font-size: 13px;"))
        .with_child(Dom::create_span_with_text(format!("{}", i + 1).as_str()).with_css("font-size: 10px;"));
    if let (true, Some(nib)) = (selected, nib) {
        dom.add_child(Dom::create_icon(nib.icon()).with_css("font-size: 12px; opacity: 0.9;"));
    }
    dom
}

/// The files, as the rows of the tree: the folders as headings, the files as buttons, the open one lit. A pick opens
/// the file and leaves the tree as it is.
fn file_list(s: &AppState, data: &RefAny) -> Dom {
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
                comp, depth, "folder", "system:accent", false, None, None, data,
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
            s.chips.file_state(&f.display),
            data,
        ));
    }
    list
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

    // The panels: their keys (Mod+B, Mod+Shift+B) are heard by the window, not by the menu, which would make a
    // second toggle of the same press.
    let mut view = StringMenuItem::create(label("azreview-menu-view"));
    view = view.with_child(MenuItem::String(
        StringMenuItem::create(label("azreview-files-switch")).with_callback(data.clone(), crate::on_toggle_sidebar),
    ));
    view = view.with_child(MenuItem::String(
        StringMenuItem::create(label("azreview-right-bar")).with_callback(data.clone(), crate::on_toggle_right),
    ));
    items.push(MenuItem::String(view));

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
    mark: Option<crate::chips::State>,
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
    row.add_child(Dom::create_span_with_text(label).with_css("flex-grow: 1; min-width: 0px; overflow: hidden;"));
    // What the file's conversations come to: the spinner for an agent at work, yellow for a suggestion that waits, green
    // when all are done; a file nobody talked about has no mark.
    if let Some(state) = mark {
        let id = match state {
            crate::chips::State::Working => ids::FILE_MARK_WORKING,
            crate::chips::State::Suggested => ids::FILE_MARK_SUGGESTED,
            crate::chips::State::Done => ids::FILE_MARK_DONE,
        };
        row.add_child(state_mark(state, 10).with_id(id).with_accessibility_name(state_word(state)));
    }
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

/// The page numbers, one under the other at the left of the pages, without a scrollbar - the one in view lit, a
/// click scrolls the pages to it.
fn page_strip(s: &AppState, data: &RefAny) -> Dom {
    let mut strip = Dom::create_div().with_id(ids::PAGES).with_css(
        format!(
            "display: flex; flex-direction: column; align-items: center; gap: 3px; width: {}px; flex-shrink: 0; \
             padding: 6px 0px; overflow-y: auto; overflow-x: hidden; scrollbar-width: none; \
             border-right: 1px solid system:separator; user-select: none;",
            crate::layout::STRIP_W
        )
        .as_str(),
    );
    if let Some(file) = s.file() {
        for page in 0..file.page_count() {
            strip.add_child(page_pill(page, page == s.visible_page, data));
        }
    }
    strip
}

/// The tools at the top bar's right end: the voice meter while recording, record, save and the settings, all one
/// height.
fn tools(s: &AppState, data: &RefAny) -> Vec<Dom> {
    let mut out = Vec::new();
    let rec = s.recording.is_some();
    if rec {
        out.push(meter(s));
        // The capture runs while it is mounted: its frames are the clip's samples.
        out.push(microphone(data));
    }
    let (icon, name) = if rec {
        ("fiber_manual_record", "azreview-stop-recording")
    } else {
        ("mic", "azreview-record")
    };
    out.push(tool(icon, label(name), rec, data, crate::on_toggle_record).with_id(ids::RECORD));
    out.push(tool("save", label("azreview-save"), false, data, crate::on_save_button).with_id(ids::SAVE));
    out.push(
        tool("settings", label("azreview-settings"), false, data, crate::on_settings_open)
            .with_id(ids::SETTINGS),
    );
    out
}

/// The thin bar at the window's right edge: its key opens and closes the whiteboard's panel.
fn right_bar(data: &RefAny) -> Dom {
    let name = label("azreview-right-bar");
    Dom::create_div()
        .with_id(ids::RIGHT_BAR)
        .with_css(
            format!(
                "display: flex; flex-direction: column; align-items: center; width: {}px; flex-shrink: 0; \
                 padding-top: 6px; border-left: 1px solid system:separator; user-select: none;",
                crate::layout::RIGHT_BAR_W
            )
            .as_str(),
        )
        .with_child(
            with_tip(key(QUIET, name.clone(), data.clone(), crate::on_toggle_right), &name)
                .with_id(ids::RIGHT_TOGGLE)
                .with_child(Dom::create_icon("dashboard").with_css("font-size: 14px;")),
        )
}

/// The right panel: the whiteboard's place, `width` wide while it slides, its edge to be dragged. It holds a word
/// about what it will be until the whiteboard is mounted in it.
fn right_panel(width: f32, data: &RefAny) -> Dom {
    Dom::create_div()
        .with_id(ids::RIGHT_PANEL)
        .with_css(
            format!(
                "position: relative; display: flex; flex-direction: column; width: {width:.0}px; flex-shrink: 0; \
                 overflow: hidden; background: system:window-background; border-left: 1px solid system:separator;"
            )
            .as_str(),
        )
        .with_child(
            Dom::create_div()
                .with_css("padding: 16px; font-size: 12px; color: system:secondary-text;")
                .with_child(Dom::create_span_with_text(label("azreview-whiteboard-placeholder"))),
        )
        .with_child(grip(crate::layout::Panel::Right, "left: 0px;", data))
}

/// The bottom bar: the conversations of the open file, one chip each (a blue spinner while its agent works, yellow
/// when it has suggested something, green when it is done); a click goes to where it is anchored. Many chips scroll
/// along the bar without a scrollbar.
fn bottom_bar(s: &AppState, data: &RefAny) -> Dom {
    let mut bar = Dom::create_div().with_id(ids::BOTTOM_BAR).with_css(
        format!(
            "display: flex; flex-direction: row; align-items: center; gap: 6px; height: {}px; \
             flex-shrink: 0; padding: 0px 10px; border-top: 1px solid system:separator; font-size: 11px; \
             color: system:secondary-text; user-select: none; overflow-x: auto; overflow-y: hidden; \
             scrollbar-width: none;",
            crate::layout::BOTTOM_H
        )
        .as_str(),
    );
    let chips = s.open_chips();
    if chips.is_empty() {
        bar.add_child(Dom::create_span_with_text(label("azreview-agents-none")));
    }
    for (i, chip) in chips.into_iter().enumerate() {
        bar.add_child(chip_key(i, chip, data));
    }
    bar
}

/// The colour of a conversation's state: blue while it works, yellow when a suggestion waits, green when done.
const fn state_colour(state: crate::chips::State) -> &'static str {
    match state {
        crate::chips::State::Working => "#0a84ff",
        crate::chips::State::Suggested => "#e5a800",
        crate::chips::State::Done => "#2fa84f",
    }
}

/// What a state is called for the screen reader and the tooltip.
fn state_word(state: crate::chips::State) -> AzString {
    label(match state {
        crate::chips::State::Working => "azreview-chip-working",
        crate::chips::State::Suggested => "azreview-chip-suggested",
        crate::chips::State::Done => "azreview-chip-done",
    })
}

/// The mark of a state: the macOS spinner while an agent works, a dot in the colour of the state otherwise.
fn state_mark(state: crate::chips::State, size: isize) -> Dom {
    match state {
        crate::chips::State::Working => Spinner::with_size(size)
            .with_color(ColorU { r: 10, g: 132, b: 255, a: 255 })
            .with_indicator(SpinnerStyle::Spokes)
            .dom(),
        _ => Dom::create_div().with_css(
            format!(
                "width: {size}px; height: {size}px; border-radius: {}px; background: {}; flex-shrink: 0;",
                size / 2,
                state_colour(state)
            )
            .as_str(),
        ),
    }
}

/// A conversation's chip: its mark and the first words, a key named by the words and the state; `i` is its place among
/// the open file's conversations.
fn chip_key(i: usize, chip: &crate::chips::Chip, data: &RefAny) -> Dom {
    let state = state_word(chip.state);
    let title = if chip.title.is_empty() { AzString::from(format!("{}", chip.id).as_str()) } else { AzString::from(chip.title.as_str()) };
    let name = AzString::from(format!("{}: {}", title.as_str(), state.as_str()).as_str());
    with_tip(
        key(
            "padding: 0px 8px; gap: 6px; background: system:control-background; border: 1px solid \
             system:separator; color: system:text; max-width: 220px;",
            name.clone(),
            data.clone(),
            crate::on_chip_click,
        ),
        &name,
    )
    .with_dataset(OptionRefAny::Some(RefAny::new(IndexTag { index: i })))
    .with_child(state_mark(chip.state, 12))
    .with_child(Dom::create_span_with_text(title.as_str()).with_css("overflow: hidden; white-space: nowrap;"))
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

/// Scrolls the page numbers' column so page `page`'s number is in view. Its place in the column is the heights of the
/// numbers before it and their gaps.
pub fn reveal_pill(info: &mut CallbackInfo, page: usize) {
    const GAP: f32 = 3.0;
    let dom = info.get_hit_node().dom;
    let column = info.get_node_id_by_id_attribute(dom, ids::PAGES_NAME);
    let Some(index) = column.into_raw().checked_sub(1) else {
        return;
    };
    let column_node = DomNodeId { dom, node: column };
    let Some(view) = info.get_node_rect(column_node).into_option() else {
        return;
    };
    let mut top = 0.0;
    let mut pill = info.get_first_child(column_node).into_option();
    for _ in 0..page {
        let Some(p) = pill else {
            return;
        };
        top += info.get_node_rect(p).into_option().map_or(0.0, |r| r.size.height) + GAP;
        pill = info.get_next_sibling(p).into_option();
    }
    let Some(height) = pill.and_then(|p| info.get_node_rect(p).into_option()).map(|r| r.size.height)
    else {
        return;
    };
    let scrolled = info
        .get_scroll_offset_for_node(dom, NodeId::create(index))
        .into_option()
        .map_or(0.0, |offset| offset.y);
    let target = if top < scrolled {
        top
    } else if top + height > scrolled + view.size.height {
        top + height - view.size.height
    } else {
        return;
    };
    info.scroll_to(dom, column, LogicalPosition::create(0.0, target.max(0.0)));
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
                    "display: flex; flex-direction: row; align-items: center; align-self: center; gap: 10px; \
                     height: 44px; padding: 0px 22px; font-size: 15px; border-radius: 8px; border: 0px solid \
                     transparent; background: system:accent; color: system:accent-text; cursor: pointer; \
                     margin-top: 6px;",
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

    sheet.add_child(page_code_column(first_line, lines));
    // The markers of the dictated notes: over the code, under the pen layer (they take no stroke).
    for marker in column::markers(notes, page, listening) {
        sheet.add_child(marker);
    }
    sheet.add_child(ink_layer(data, page, Layer::Pen));
    sheet
}

/// The code of a page as the page shows it: a row per line, its number in the gutter and then the line, a column
/// `CODE_W` wide whose rows are `LINE_H` tall. The whiteboard's cutout renders this same column for the lines it holds,
/// so that it looks like the page it was taken from.
#[must_use]
pub fn page_code_column(first_line: usize, lines: &[String]) -> Dom {
    let mut col = Dom::create_div().with_css(
        format!(
            "position: relative; display: flex; flex-direction: column; width: {}px; overflow: \
             hidden; user-select: none;",
            CODE_W as isize,
        )
        .as_str(),
    );
    for (i, line) in lines.iter().enumerate() {
        col.add_child(code_row(first_line + i, line.as_str()));
    }
    col
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

    /// No title bar: the top bar moves the window (macOS's traffic lights
    /// in it), and what sits in it stays pressable.
    #[test]
    fn the_top_bar_moves_the_window_and_its_controls_stay_controls() {
        let s = app(2);
        let row = top_bar(&s, &RefAny::new(0u8), 0.0);
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
    fn the_top_bar_holds_the_five_meanings_and_the_files_switch_is_over_the_tree() {
        let s = app(2);
        let row = top_bar(&s, &RefAny::new(0u8), 0.0);
        let shown = texts(&row);
        for digit in ["1", "2", "3", "4", "5"] {
            assert!(shown.iter().any(|t| t == digit), "meaning {digit} in {shown:?}");
        }
        assert!(!shown.iter().any(|t| t == "AzReview"), "no title: {shown:?}");
        let view = main_view(&s, &RefAny::new(0u8), 1280.0);
        assert!(with_id(&view, &ids::SIDEBAR_TOGGLE).is_some(), "the files' switch, next to the traffic lights");
    }

    #[test]
    fn the_window_has_no_status_bar() {
        let s = app(2);
        let view = main_view(&s, &RefAny::new(0u8), 1280.0);
        assert!(nodes(&view)
            .iter()
            .all(|n| !n.root.has_class(azul::str::String::from("__azul-native-statusbar"))));
    }

    #[test]
    fn the_tree_is_a_column_beside_the_pages_that_pushes_them_and_has_no_heading() {
        let s = app(2);
        let view = main_view(&s, &RefAny::new(0u8), 1280.0);
        let tree = with_id(&view, &ids::TREE).expect("the file tree");
        assert!(!declares(tree, "Position(Exact(Absolute))"), "beside the pages, not over them");
        let shown = texts(tree);
        assert!(shown.iter().any(|t| t == "f1.rs"), "it lists the files: {shown:?}");
        assert!(!shown.iter().any(|t| t == "Files"), "no heading, it takes room: {shown:?}");
        // the tree is the first of the window's columns, the pages' column after it
        let root_children: Vec<_> = view.children.as_slice().iter().map(|c| c.root.has_id(ids::TREE.clone())).collect();
        assert_eq!(root_children.first(), Some(&true), "the tree comes first (the pages are pushed to its right)");
    }

    #[test]
    fn a_closed_tree_is_not_in_the_window_and_the_switch_still_is() {
        let mut s = app(2);
        s.panels.tree_open = false;
        s.slide = crate::layout::Slide::settled(&s.panels);
        let view = main_view(&s, &RefAny::new(0u8), 1280.0);
        assert!(with_id(&view, &ids::TREE).is_none());
        assert!(with_id(&view, &ids::SIDEBAR_TOGGLE).is_some());
    }

    #[test]
    fn where_there_is_no_room_to_push_the_tree_covers_the_pages() {
        let s = app(2);
        let view = main_view(&s, &RefAny::new(0u8), 600.0);
        let tree = with_id(&view, &ids::TREE).expect("the file tree");
        assert!(declares(tree, "Position(Exact(Absolute))"), "over the pages");
    }

    #[test]
    fn picking_a_file_opens_it_and_leaves_the_tree_as_it_is() {
        let mut s = app(2);
        assert!(s.panels.tree_open, "premise: the tree is out");
        s.open_file(1);
        assert_eq!(s.current, Some(1));
        assert!(s.panels.tree_open, "a pick does not fold the tree");
    }

    /// An icon is enough for record, save and settings; the name is for the
    /// screen reader and the tooltip.
    #[test]
    fn record_save_and_settings_are_icon_only_buttons() {
        let s = app(1);
        let rail = top_bar(&s, &RefAny::new(0u8), 0.0);
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
        let s = app(2);
        let view = main_view(&s, &RefAny::new(0u8), 1280.0);
        for n in nodes(&view) {
            // A panel's edge is dragged, not pressed: it takes the pointer and no focus.
            if n.root.has_id(ids::TREE_GRIP.clone()) || n.root.has_id(ids::RIGHT_GRIP.clone()) {
                continue;
            }
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
        let view = main_view(&s, &RefAny::new(0u8), 1280.0);
        assert!(texts(&view).iter().any(|t| t.contains("the disk is full")));

        s.problem = None;
        let view = main_view(&s, &RefAny::new(0u8), 1280.0);
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
