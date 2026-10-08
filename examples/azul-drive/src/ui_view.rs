//! The content pane: a LEAF on the page (Finder's window in the app theme, [`crate::look`]:
//! flora's paper in a thin rule with a soft cast shadow on the linen, Office's white on silver
//! in flat) holding This PC (the drives as tiles in groups), Quick access (the pinned folders
//! and the recent places), or a folder in one of Explorer's eight layouts - Details by default
//! (the file-type icon and name, Date modified, Type, Size under sortable headers on a raised
//! face, the rows in quiet stripes), the icon layouts on azul's IconGrid - grouped or not, with
//! item check boxes or not (a selected hand-built icon cell or tile lifts off the paper) -
//! with the InfoBar over it, and Finder's path bar and status line at its foot. Every item takes
//! a click (Ctrl / Shift too), a double-click, a right-click (the context menu), a drag (a
//! folder takes the drop: a move, Ctrl a copy) and F2's rename field.
//!
//! A folder's items are a VIRTUAL view ([`folder_view`]): only the lines in view and a screen
//! either side are built (a line is a Details or Content row, or a row of cells, or a group's
//! header - every line of a layout one height, [`line_px`]), however many items the folder has;
//! a scroll draws the lines it brings in without a rebuild of the window, and when it settles
//! the rows in view get their sizes, dates, item counts and thumbnails
//! (`actions::request_view_work`). The icon layouts' IconGrid draws its viewport only and asks
//! for the items it shows ([`GridItems`]).

use std::ops::Range;

use azul::{
    callbacks::{
        AccordionOnToggleCallbackType, ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType,
        IconGridDataSourceCallbackType, IconGridOnEventCallbackType,
        TextInputOnFocusLostCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType, TileOnClickCallbackType,
    },
    dom::{AttributeType, DomId, FocusTarget, NodeId, VirtualKeyCode},
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{
        Accordion, AccordionSection, AccordionVariant, CheckBoxState, IconGrid, IconGridEvent,
        IconGridEventKind, IconGridItem, InfoBar, OnTextInputReturn, TextInputState,
        TextInputValid, Tile, TileCapacity,
    },
};
use crate::{
    actions::{self, action_ref, on_action, Action},
    browse::{self, Column, Entry, Place},
    go, ids,
    listing::{self, Line},
    look,
    model::{self, ViewLayout},
    preview::{self, PreviewKind},
    save_settings, ui_panes, with_state, ColumnDrag, DriveState, Message,
};

// ==== Shared pieces ====

/// The Material icon of an item: a folder, or the kind of file.
pub(crate) fn icon_for(entry: &Entry) -> &'static str {
    if entry.is_folder {
        return "folder";
    }
    let ext = browse::extension_of(&entry.name)
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match preview::preview_kind(&entry.name) {
        PreviewKind::Image => "image",
        PreviewKind::Video => "movie",
        PreviewKind::Audio => "music_note",
        PreviewKind::Pdf => "picture_as_pdf",
        _ => match ext.as_str() {
            "zip" | "tar" | "gz" | "7z" | "rar" => "archive",
            "rs" | "js" | "ts" | "py" | "c" | "h" | "cpp" | "go" | "java" | "sh" | "toml"
            | "json" | "html" | "css" | "xml" | "yaml" | "yml" => "code",
            "csv" | "xlsx" | "tsv" => "table_chart",
            "md" | "docx" | "rst" => "article",
            "txt" | "log" => "description",
            "eml" => "mail",
            "ics" => "event",
            _ => "insert_drive_file",
        },
    }
}

/// The colour of an item's icon, as Explorer tints its file-type icons: the folder yellow, a
/// colour per kind of file, a grey-blue for the rest.
fn icon_colour(entry: &Entry) -> &'static str {
    if entry.is_folder {
        return "color: #D9A93B;";
    }
    match icon_for(entry) {
        "image" => "color: #2E9E5B;",
        "movie" => "color: #8E44AD;",
        "music_note" => "color: #E67E22;",
        "picture_as_pdf" => "color: #D93025;",
        "archive" => "color: #A0743C;",
        "code" => "color: #3B78D8;",
        "table_chart" => "color: #1E8E3E;",
        "article" => "color: #2B579A;",
        "mail" => "color: #0F6CBD;",
        "event" => "color: #C2410C;",
        _ => "color: #6B7A8F;",
    }
}

/// Whether `entry` is a file of a cloud drive (fetched when opened): Explorer's cloud status.
fn in_the_cloud(s: &DriveState, entry: &Entry) -> bool {
    !entry.is_folder && s.current_drive_id().is_some_and(|id| !s.is_local_drive(&id))
}

/// The paint of an item ([`crate::look`]): at rest (a wash under the pointer), selected,
/// focused, cut (faded) - each state after the one before, so it wins.
fn item_paint(s: &DriveState, entry: &Entry) -> String {
    paint(s, entry, false)
}

/// The paint of an icon cell: an item's, rounded, lifting off the paper while it is
/// selected.
fn cell_paint(s: &DriveState, entry: &Entry) -> String {
    paint(s, entry, true)
}

fn paint(s: &DriveState, entry: &Entry, cell: bool) -> String {
    let mut css = String::from(look::ITEM);
    if cell {
        css.push(' ');
        css.push_str(look::CELL);
    }
    if s.selection.contains(&entry.key) {
        css.push(' ');
        css.push_str(look::ITEM_SELECTED);
        if cell {
            css.push(' ');
            css.push_str(look::CELL_SELECTED);
        }
    }
    if s.selection.focus() == Some(entry.key.as_str()) {
        css.push(' ');
        css.push_str(look::ITEM_FOCUS);
    }
    if actions::is_cut(s, &entry.key) {
        css.push_str(" opacity: 0.5;");
    }
    css
}

/// A pane's content as a leaf on the page (the right pane: the preview, the details).
pub(crate) fn on_page(content: Dom) -> Dom {
    Dom::create_div().with_css(look::PAGE).with_child(
        Dom::create_div()
            .with_css(look::LEAF)
            .with_child(content),
    )
}

/// The InfoBar over the content: the last message, with Dismiss.
fn info_bar(message: &Message, app: &RefAny) -> Dom {
    InfoBar::create(AzString::from(message.text.as_str()))
        .with_kind(message.kind.alert())
        .with_action(AzString::from("Dismiss"))
        .with_on_action(app.clone(), on_dismiss as ButtonOnClickCallbackType)
        .dom()
        .with_id(ids::INFO_BAR)
}

extern "C" fn on_dismiss(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| s.message = None)
}

/// Whether the folder shows as azul's IconGrid: an icon layout, not grouped, without item
/// check boxes, nothing being renamed in place (the grid draws its labels itself; the
/// hand-built icon cells hold the rename field), and something to show.
pub(crate) fn uses_icon_grid(s: &DriveState) -> bool {
    matches!(s.place, Place::Folder { .. })
        && matches!(
            s.settings.layout,
            ViewLayout::ExtraLargeIcons | ViewLayout::LargeIcons | ViewLayout::MediumIcons
        )
        && s.settings.group_by == model::GroupBy::None
        && !s.settings.item_checkboxes
        && s.renaming.is_none()
        && !s.visible_entries().is_empty()
}

/// The content pane: the page, the leaf on it (the InfoBar, the view, "Load more", the path bar
/// and the status line); `size` is what the view has (the icon grid draws exactly that).
pub(crate) fn content(s: &DriveState, app: &RefAny, size: (f32, f32)) -> Dom {
    let mut area = Dom::create_div().with_id(ids::LEAF).with_css(look::LEAF);
    if let Some(message) = &s.message {
        area.add_child(info_bar(message, app));
    }
    let grid = uses_icon_grid(s);
    let folder = matches!(s.place, Place::Folder { .. });
    let view = match &s.place {
        Place::ThisPc => this_pc(s, app),
        Place::QuickAccess => quick_access(s, app),
        Place::Folder { .. } if grid => icon_grid(s, app, size),
        Place::Folder { .. } => folder_view(s, app),
    };
    let background = RefAny::new(BackgroundRef { app: app.clone() });
    let mut host = Dom::create_div()
        .with_id(ids::VIEW)
        .with_css(if grid {
            // The grid scrolls itself, by whole rows.
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
             overflow: hidden;"
        } else if folder {
            // A folder's rows are a virtual view that scrolls itself; a Details header wider
            // than the leaf scrolls across with them.
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
             overflow-x: auto; overflow-y: hidden;"
        } else {
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
             overflow: auto;"
        })
        .with_child(view);
    if !grid {
        // A click on the empty space selects nothing; a right-click there is the folder's
        // menu. (The grid hit-tests its own empty space and says so in its events.)
        host = host
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                background.clone(),
                on_background_click,
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::RightMouseUp),
                background.clone(),
                on_background_menu,
            );
    }
    area.add_child(host.with_callback(
        EventFilter::Hover(HoverEventFilter::Drop),
        background,
        on_background_drop,
    ));
    area.add_child(ui_panes::path_bar(s, app));
    area.add_child(ui_panes::status_line(s, app));
    Dom::create_div()
        .with_id(ids::CONTENT)
        .with_css(look::PAGE)
        .with_child(area)
}

struct BackgroundRef {
    app: RefAny,
}

fn background_app(data: &mut RefAny) -> Option<RefAny> {
    data.downcast_ref::<BackgroundRef>().map(|b| b.app.clone())
}

extern "C" fn on_background_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut app) = background_app(&mut data) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        if s.renaming.is_some() {
            actions::commit_rename(info, app, s);
        }
        if s.current_drive().is_some() && !s.selection.is_empty() {
            s.selection.clear();
            s.print_selection();
            s.clear_preview();
        }
        s.selected_drive = None;
        s.selected_pin = None;
    })
}

extern "C" fn on_background_menu(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut app) = background_app(&mut data) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        s.selection.clear();
        let menu = actions::context_menu(app, s);
        info.open_menu_for_hit_node(menu);
    })
}

/// Items dropped on the empty space of a folder: into the open folder.
extern "C" fn on_background_drop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut app) = background_app(&mut data) else {
        return Update::DoNothing;
    };
    let copy = info.get_key_modifiers().primary_down();
    with_state(&mut app, &mut info, |info, app, s| {
        let prefix = s.prefix().to_string();
        actions::drop_on_folder(info, app, s, &prefix, copy);
    })
}

// ==== Groups ====

/// What a group accordion carries: the app and its groups' labels.
struct GroupsRef {
    app: RefAny,
    labels: Vec<String>,
}

/// Collapsible groups (Explorer's group headers) of `sections`
/// ((label, count, content)).
fn groups(s: &DriveState, app: &RefAny, sections: Vec<(String, usize, Dom)>) -> Dom {
    let labels: Vec<String> = sections.iter().map(|(l, _, _)| l.clone()).collect();
    let sections: Vec<AccordionSection> = sections
        .into_iter()
        .map(|(label, count, content)| {
            let open = !s.groups_closed.contains(&label);
            AccordionSection::create(AzString::from(label.as_str()), content)
                .with_count(count)
                .with_open(open)
        })
        .collect();
    Accordion::create_with_sections(sections)
        .with_variant(AccordionVariant::Groups)
        .with_on_toggle(
            RefAny::new(GroupsRef {
                app: app.clone(),
                labels,
            }),
            on_group_toggle as AccordionOnToggleCallbackType,
        )
        .dom()
}

extern "C" fn on_group_toggle(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some((mut app, label)) = data
        .downcast_ref::<GroupsRef>()
        .and_then(|g| Some((g.app.clone(), g.labels.get(index)?.clone())))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |_info, _app, s| {
        if !s.groups_closed.remove(&label) {
            s.groups_closed.insert(label);
        }
    })
}

/// A wrapping row of cells.
fn wrap_row(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; flex-wrap: wrap; align-content: flex-start; \
             padding: 6px 8px;",
        )
        .with_children(DomVec::from(children))
}

// ==== This PC and Quick access ====

/// A drive tile's or pin's click data.
struct PlaceRef {
    app: RefAny,
    /// A drive (This PC) or a pin (Quick access) by index; or a place.
    drive: Option<usize>,
    pin: Option<usize>,
    place: Place,
}

fn place_ref(app: &RefAny, drive: Option<usize>, pin: Option<usize>, place: Place) -> RefAny {
    RefAny::new(PlaceRef {
        app: app.clone(),
        drive,
        pin,
        place,
    })
}

extern "C" fn on_place_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, drive, pin)) = data
        .downcast_ref::<PlaceRef>()
        .map(|r| (r.app.clone(), r.drive, r.pin))
    else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    with_state(&mut app, &mut info, |_info, _app, s| {
        if let Some(index) = drive {
            actions::select_drive(s, index);
        }
        s.selected_pin = pin;
    })
}

extern "C" fn on_place_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, place)) = data
        .downcast_ref::<PlaceRef>()
        .map(|r| (r.app.clone(), r.place.clone()))
    else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    with_state(&mut app, &mut info, |info, app, s| go(info, app, s, place, true))
}

fn drive_tile(s: &DriveState, app: &RefAny, index: usize) -> Dom {
    let slot = &s.slots[index];
    let mut tile = Tile::create(AzString::from(slot.entry.name.as_str()))
        .with_icon(AzString::from(slot.icon()))
        .with_selected(s.selected_drive == Some(index));
    match (s.disk.get(&slot.entry.id), s.root_counts.get(&slot.entry.id)) {
        (Some((total, free)), _) => tile = tile.with_capacity(TileCapacity::create(*total, *free)),
        (None, Some(count)) => {
            tile = tile.with_detail(AzString::from(format!(
                "{}, {} at the root",
                slot.kind(),
                browse::counted(*count, "item", "items")
            )));
        }
        (None, None) => tile = tile.with_detail(AzString::from(slot.kind())),
    }
    let place = Place::folder(&slot.entry.id, "");
    let data = place_ref(app, Some(index), None, place);
    tile.with_on_click(data.clone(), on_place_click as TileOnClickCallbackType)
        .with_on_double_click(data, on_place_open as TileOnClickCallbackType)
        .dom()
        .with_class(ids::DRIVE_CLASS)
        .with_css(tile_css(260, s.selected_drive == Some(index)))
}

/// A Tile's place in a wrapping row: its width, the room around it, the lift of a selected one.
fn tile_css(width: u32, selected: bool) -> String {
    format!(
        "width: {width}px; margin: 0px 8px 8px 0px; {}",
        if selected { look::TILE_SELECTED } else { "" }
    )
}

/// This PC: the drives, local and in the cloud, as tiles in groups.
fn this_pc(s: &DriveState, app: &RefAny) -> Dom {
    let local: Vec<usize> = (0..s.slots.len()).filter(|i| s.slots[*i].is_local()).collect();
    let cloud: Vec<usize> = (0..s.slots.len()).filter(|i| !s.slots[*i].is_local()).collect();
    let tiles = |indices: &[usize]| wrap_row(indices.iter().map(|i| drive_tile(s, app, *i)).collect());
    let mut sections = vec![(
        String::from("Devices and drives"),
        local.len(),
        tiles(&local),
    )];
    sections.push((
        String::from("Network locations"),
        cloud.len(),
        if cloud.is_empty() {
            ShellEmptyState::create(AzString::from("No cloud drive yet."))
                .with_icon(AzString::from("cloud_queue"))
                .with_detail(AzString::from(
                    "Add an AWS S3, Cloudflare R2 or MinIO bucket; its keys stay in the system \
                     keyring.",
                ))
                .with_action_label(AzString::from("Add S3 drive"))
                .with_on_action(action_ref(app, Action::AddDrive), on_action as ButtonOnClickCallbackType)
                .dom()
        } else {
            tiles(&cloud)
        },
    ));
    groups(s, app, sections).with_id(ids::THIS_PC)
}

/// Quick access: the pinned folders and the places visited last.
fn quick_access(s: &DriveState, app: &RefAny) -> Dom {
    let pins: Vec<Dom> = s
        .settings
        .pinned
        .iter()
        .enumerate()
        .map(|(i, pin)| {
            let place = Place::folder(&pin.drive, &pin.prefix);
            let data = place_ref(app, None, Some(i), place.clone());
            Tile::create(AzString::from(pin.name.as_str()))
                .with_icon(AzString::from("folder_special"))
                .with_detail(AzString::from(browse::path_text(
                    &place,
                    Some(&s.drive_name(&place)),
                )))
                .with_selected(s.selected_pin == Some(i))
                .with_on_click(data.clone(), on_place_click as TileOnClickCallbackType)
                .with_on_double_click(data, on_place_open as TileOnClickCallbackType)
                .dom()
                .with_css(tile_css(250, s.selected_pin == Some(i)))
        })
        .collect();
    let pinned = if pins.is_empty() {
        ShellEmptyState::create(AzString::from("Nothing is pinned yet."))
            .with_icon(AzString::from("push_pin"))
            .with_detail(AzString::from(
                "Open a folder and choose See more (...) > Pin to Quick access.",
            ))
            .dom()
    } else {
        wrap_row(pins)
    };
    let recent: Vec<Dom> = s
        .recent
        .iter()
        .filter(|p| matches!(p, Place::Folder { .. }))
        .take(8)
        .map(|place| {
            let data = place_ref(app, None, None, place.clone());
            Tile::create(AzString::from(s.place_title(place)))
                .with_icon(AzString::from("history"))
                .with_detail(AzString::from(browse::path_text(
                    place,
                    Some(&s.drive_name(place)),
                )))
                .with_on_double_click(data, on_place_open as TileOnClickCallbackType)
                .dom()
                .with_css(tile_css(250, false))
        })
        .collect();
    let recent_count = recent.len();
    groups(
        s,
        app,
        vec![
            (String::from("Pinned folders"), s.settings.pinned.len(), pinned),
            (String::from("Recent places"), recent_count, wrap_row(recent)),
        ],
    )
    .with_id(ids::QUICK_ACCESS)
}

// ==== A folder ====

/// An item's data for its callbacks.
struct ItemRef {
    app: RefAny,
    key: String,
    is_folder: bool,
}

fn item_ref(app: &RefAny, entry: &Entry) -> RefAny {
    RefAny::new(ItemRef {
        app: app.clone(),
        key: entry.key.clone(),
        is_folder: entry.is_folder,
    })
}

fn item_parts(data: &mut RefAny) -> Option<(RefAny, String, bool)> {
    data.downcast_ref::<ItemRef>()
        .map(|r| (r.app.clone(), r.key.clone(), r.is_folder))
}

/// An item takes clicks, a double-click, the right button, a drag and (a
/// folder) a drop. An item being renamed does not drag: a drag in its field
/// selects text.
fn interactive(s: &DriveState, app: &RefAny, entry: &Entry, dom: Dom) -> Dom {
    let data = item_ref(app, entry);
    let renaming = s.renaming.as_ref().is_some_and(|r| r.key == entry.key);
    let mut dom = dom
        .with_class(ids::ITEM_CLASS)
        .with_attribute(AttributeType::Draggable(!renaming))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            data.clone(),
            on_item_click,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::DoubleClick),
            data.clone(),
            on_item_open,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::RightMouseUp),
            data.clone(),
            on_item_menu,
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::DragStart),
            data.clone(),
            on_item_drag,
        );
    if entry.is_folder {
        dom = dom
            .with_class(ids::FOLDER_CLASS)
            .with_callback(EventFilter::Hover(HoverEventFilter::Drop), data, on_item_drop);
    }
    dom
}

extern "C" fn on_item_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, key, _)) = item_parts(&mut data) else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    let mods = info.get_key_modifiers();
    let toggle = mods.primary_down();
    let range = mods.shift;
    with_state(&mut app, &mut info, |info, app, s| {
        if s.renaming.as_ref().is_some_and(|r| r.key == key) {
            return;
        }
        actions::click_item(info, app, s, &key, toggle, range);
    })
}

extern "C" fn on_item_open(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, key, _)) = item_parts(&mut data) else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    with_state(&mut app, &mut info, |info, app, s| {
        if s.renaming.as_ref().is_some_and(|r| r.key == key) {
            return;
        }
        actions::activate(info, app, s, &key);
    })
}

extern "C" fn on_item_menu(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, key, _)) = item_parts(&mut data) else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    with_state(&mut app, &mut info, |info, app, s| {
        if !s.selection.contains(&key) {
            actions::click_item(info, app, s, &key, false, false);
        }
        let menu = actions::context_menu(app, s);
        info.open_menu_for_hit_node(menu);
    })
}

/// A drag starts on an item: the selection goes along (or the item alone
/// when it is not selected). No rebuild while the drag runs.
extern "C" fn on_item_drag(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, key, is_folder)) = item_parts(&mut data) else {
        return Update::DoNothing;
    };
    if let Some(mut s) = app.downcast_mut::<DriveState>() {
        let Some(drive) = s.current_drive_id() else {
            return Update::DoNothing;
        };
        let items = if s.selection.contains(&key) {
            s.selected_items()
        } else {
            let size = s.entry(&key).and_then(|e| e.size);
            vec![crate::fileops::SourceItem {
                key: key.clone(),
                is_folder,
                size,
            }]
        };
        s.dragging = Some((drive, items));
    }
    Update::DoNothing
}

/// Items dropped on a folder: moved into it (Ctrl: copied).
extern "C" fn on_item_drop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, key, is_folder)) = item_parts(&mut data) else {
        return Update::DoNothing;
    };
    if !is_folder {
        return Update::DoNothing;
    }
    info.stop_propagation();
    let mods = info.get_key_modifiers();
    let copy = mods.primary_down();
    with_state(&mut app, &mut info, |info, app, s| {
        actions::drop_on_folder(info, app, s, &key, copy);
    })
}

/// An item's check box ("Item check boxes"): in or out of the selection.
extern "C" fn on_item_check(mut data: RefAny, mut info: CallbackInfo, _state: CheckBoxState) -> Update {
    let Some((mut app, key, _)) = item_parts(&mut data) else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    with_state(&mut app, &mut info, |info, app, s| {
        actions::click_item(info, app, s, &key, true, false);
    })
}

fn check_box(s: &DriveState, app: &RefAny, entry: &Entry) -> Dom {
    CheckBox::create(s.selection.contains(&entry.key))
        .with_on_toggle(item_ref(app, entry), on_item_check as CheckBoxOnToggleCallbackType)
        .dom()
        .with_css("margin-right: 6px; flex-shrink: 0;")
}

/// F2's field: the name being typed; Enter or a click elsewhere renames,
/// Escape gives up.
fn rename_field(s: &DriveState, app: &RefAny) -> Dom {
    let text = s
        .renaming
        .as_ref()
        .map(|r| r.text.clone())
        .unwrap_or_default();
    TextInput::create()
        .with_text(AzString::from(text))
        .with_on_text_input(app.clone(), on_rename_text as TextInputOnTextInputCallbackType)
        .with_on_virtual_key_down(app.clone(), on_rename_key as TextInputOnVirtualKeyDownCallbackType)
        .with_on_focus_lost(app.clone(), on_rename_blur as TextInputOnFocusLostCallbackType)
        .with_accessibility_name(AzString::from("New name"))
        .dom()
        .with_id(ids::RENAME_FIELD)
        .with_css("min-width: 120px; flex-grow: 1;")
        .with_callback(
            EventFilter::Component(ComponentEventFilter::AfterMount),
            app.clone(),
            on_rename_mounted,
        )
}

extern "C" fn on_rename_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<DriveState>() {
        if let Some(renaming) = s.renaming.as_mut() {
            renaming.text = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_rename_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let update = match key {
        Some(VirtualKeyCode::Return) | Some(VirtualKeyCode::NumpadEnter) => {
            info.stop_propagation();
            commit_rename_with(&mut data, &mut info, &state)
        }
        Some(VirtualKeyCode::Escape) => {
            info.stop_propagation();
            with_state(&mut data, &mut info, |_info, _app, s| s.renaming = None)
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_rename_blur(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> Update {
    commit_rename_with(&mut data, &mut info, &state)
}

/// Renames to the field's text as the widget hands it to its key and blur hooks (the engine's
/// buffer, Backspace and Delete included). `on_rename_text` only sees typing: committing from
/// its copy lost every deletion since the last character typed (`notes.txt`, End, nine
/// Backspaces, `todo.txt` renamed to `todo.txtn`).
fn commit_rename_with(data: &mut RefAny, info: &mut CallbackInfo, state: &TextInputState) -> Update {
    let text = state.get_text().as_str().to_string();
    with_state(data, info, |info, app, s| {
        if let Some(renaming) = s.renaming.as_mut() {
            renaming.text = text;
        }
        actions::commit_rename(info, app, s)
    })
}

/// The rename field appeared: it takes the keyboard.
extern "C" fn on_rename_mounted(_data: RefAny, mut info: CallbackInfo) -> Update {
    let node = info.get_hit_node();
    info.set_focus(FocusTarget::Id(node));
    Update::DoNothing
}

/// An item's icon and name (or the rename field), in a row.
fn name_cell(s: &DriveState, app: &RefAny, entry: &Entry, icon_px: f32) -> Dom {
    let icon = Dom::create_icon(AzString::from(icon_for(entry))).with_css(format!(
        "font-size: {icon_px}px; margin-right: 6px; flex-shrink: 0; {}",
        icon_colour(entry)
    ));
    let renaming = s.renaming.as_ref().is_some_and(|r| r.key == entry.key);
    let label = if renaming {
        rename_field(s, app)
    } else {
        Dom::create_span_with_text(AzString::from(
            entry.display_name(s.settings.show_extensions),
        ))
        .with_class(ids::NAME_CLASS)
        .with_css("overflow: hidden; text-overflow: ellipsis; white-space: nowrap;")
    };
    let mut cell = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; min-width: 0px;")
        .with_child(icon)
        .with_child(label);
    if !renaming && in_the_cloud(s, entry) {
        // Explorer's status: a cloud file, fetched when it is opened.
        cell.add_child(
            Dom::create_icon(AzString::from("cloud_queue"))
                .with_css("font-size: 14px; margin-left: 6px; opacity: 0.55; flex-shrink: 0;"),
        );
    }
    cell
}

/// What a column's edge carries.
struct ColumnRef {
    app: RefAny,
    column: Column,
}

fn column_parts(data: &mut RefAny) -> Option<(RefAny, Column)> {
    data.downcast_ref::<ColumnRef>()
        .map(|c| (c.app.clone(), c.column))
}

/// The Details header: the columns on a raised face (a click sorts, again reverses; the sorted
/// one tinted, its arrow beside its name), each with an edge to drag (its width) or
/// double-click (to fit).
fn details_header(s: &DriveState, app: &RefAny) -> Dom {
    let mut row = Dom::create_div()
        .with_id(ids::DETAILS_HEADER)
        .with_css(look::DETAILS_HEADER);
    if s.settings.item_checkboxes {
        row.add_child(Dom::create_div().with_css("width: 28px; flex-shrink: 0;"));
    }
    for c in &s.settings.columns.columns {
        let width = c.width;
        let sorted = s.settings.sort.column == c.column;
        let data = RefAny::new(ColumnRef {
            app: app.clone(),
            column: c.column,
        });
        let mut cell = Dom::create_div()
            .with_class(ids::COLUMN_CLASS)
            .with_css(format!(
                "{} width: {width}px; min-width: {width}px; {}",
                look::COLUMN,
                if sorted { look::COLUMN_SORTED } else { "" }
            ))
            .with_child(
                Dom::create_span_with_text(AzString::from(c.column.label())).with_css(
                    "flex-grow: 1; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;",
                ),
            );
        if sorted {
            let arrow = if s.settings.sort.descending {
                "arrow_drop_down"
            } else {
                "arrow_drop_up"
            };
            cell.add_child(
                Dom::create_icon(AzString::from(arrow))
                    .with_css("font-size: 16px; flex-shrink: 0;"),
            );
        }
        cell.add_child(
            Dom::create_div()
                .with_class(ids::COLUMN_EDGE_CLASS)
                .with_css(look::COLUMN_EDGE)
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::LeftMouseDown),
                    data.clone(),
                    on_column_drag_start,
                )
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    data.clone(),
                    on_column_edge_click,
                )
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::DoubleClick),
                    data.clone(),
                    on_column_edge_fit,
                ),
        );
        row.add_child(cell.with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            data,
            on_column_click,
        ));
    }
    row
}

extern "C" fn on_column_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, column)) = column_parts(&mut data) else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    with_state(&mut app, &mut info, |info, app, s| {
        actions::sort_by(info, app, s, column, None)
    })
}

/// A click on a column's edge is not a sort.
extern "C" fn on_column_edge_click(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    Update::DoNothing
}

/// A double-click on a column's edge: the column fits its widest text.
extern "C" fn on_column_edge_fit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, column)) = column_parts(&mut data) else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    with_state(&mut app, &mut info, |info, app, s| {
        let show_extensions = s.settings.show_extensions;
        let entries: Vec<Entry> = s.visible_entries().into_iter().cloned().collect();
        let refs: Vec<&Entry> = entries.iter().collect();
        let mut fitted = s.settings.columns.clone();
        fitted.fit(&refs, show_extensions);
        let width = fitted.width(column);
        s.settings.columns.resize(column, width);
        save_settings(info, app, s);
    })
}

/// The mouse went down on a column's edge: the drag starts.
extern "C" fn on_column_drag_start(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, column)) = column_parts(&mut data) else {
        return Update::DoNothing;
    };
    let Some(at) = info.get_cursor_position().into_option() else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    with_state(&mut app, &mut info, |_info, _app, s| {
        s.column_drag = Some(ColumnDrag {
            column,
            start_x: at.x,
            start_width: s.settings.columns.width(column),
        });
    })
}

/// The mouse moves while a column's edge is held: the column follows it.
pub(crate) extern "C" fn on_column_drag_move(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(at) = info.get_cursor_position().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let Some(drag) = s.column_drag else {
        return Update::DoNothing;
    };
    let width = drag.start_width + (at.x - drag.start_x);
    if (s.settings.columns.width(drag.column) - width).abs() < 1.0 {
        return Update::DoNothing;
    }
    s.settings.columns.resize(drag.column, width);
    Update::RefreshDom
}

/// The mouse went up: the column keeps its width (saved).
pub(crate) extern "C" fn on_column_drag_end(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if s.column_drag.take().is_some() {
            println!(
                "AZDRIVE_COLUMNS {}",
                s.settings
                    .columns
                    .columns
                    .iter()
                    .map(|c| format!("{}={:.0}", c.column.label(), c.width))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            save_settings(info, app, s);
        }
    })
}

/// A row of the Details layout; every other one (`alt`) is a quiet stripe (Finder's list),
/// under the hover and the selection.
fn details_row(s: &DriveState, app: &RefAny, entry: &Entry, alt: bool) -> Dom {
    let shade = if alt { look::STRIPE } else { "" };
    // Finder's dense rows, all of one height: the virtual view places a row by its position.
    let mut row = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; align-items: center; height: {}px; \
         box-sizing: border-box; flex-shrink: 0; {shade} {}",
        line_px(s, ViewLayout::Details),
        item_paint(s, entry)
    ));
    if alt {
        row.add_class(ids::ROW_ALT_CLASS);
    }
    if s.settings.item_checkboxes {
        row.add_child(check_box(s, app, entry));
    }
    for c in &s.settings.columns.columns {
        let width = c.width;
        let (cell, extra) = match c.column {
            Column::Name => (name_cell(s, app, entry, 16.0), ""),
            Column::Size => (
                Dom::create_span_with_text(AzString::from(size_text(s, entry))),
                "opacity: 0.8; display: flex; flex-direction: row; justify-content: flex-end; \
                 padding-right: 12px;",
            ),
            _ => (
                Dom::create_span_with_text(AzString::from(model::cell_text(
                    entry,
                    c.column,
                    s.settings.show_extensions,
                ))),
                "opacity: 0.8;",
            ),
        };
        row.add_child(
            Dom::create_div()
                .with_css(format!(
                    "width: {width}px; min-width: {width}px; flex-shrink: 0; padding-left: 8px; \
                     overflow: hidden; white-space: nowrap; text-overflow: ellipsis; {extra}"
                ))
                .with_child(cell),
        );
    }
    interactive(s, app, entry, row)
}

/// A row of the Content layout: icon, name and type, date and size.
fn content_row(s: &DriveState, app: &RefAny, entry: &Entry) -> Dom {
    let mut row = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; align-items: center; height: {}px; \
         box-sizing: border-box; padding: 0px 8px; flex-shrink: 0; {}",
        line_px(s, ViewLayout::Content),
        item_paint(s, entry)
    ));
    if s.settings.item_checkboxes {
        row.add_child(check_box(s, app, entry));
    }
    let size = if entry.is_folder {
        s.counts
            .get(&entry.key)
            .map(|n| browse::counted(*n, "item", "items"))
            .unwrap_or_default()
    } else if entry.known {
        format!("Size: {}", browse::format_size(entry.size))
    } else {
        String::new()
    };
    let modified = browse::format_modified(entry.modified, &chrono::Local);
    let modified = if modified.is_empty() {
        String::new()
    } else {
        format!("Date modified: {modified}")
    };
    row.add_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px;")
            .with_child(name_cell(s, app, entry, 32.0))
            .with_child(
                Dom::create_span_with_text(AzString::from(entry.kind()))
                    .with_css("font-size: 12px; opacity: 0.7; margin-left: 38px;"),
            ),
    );
    row.add_child(
        Dom::create_div()
            .with_css(
                "display: flex; flex-direction: column; align-items: flex-end; font-size: 12px; \
                 opacity: 0.75; flex-shrink: 0; margin-left: 12px;",
            )
            .with_child(Dom::create_span_with_text(AzString::from(modified)))
            .with_child(Dom::create_span_with_text(AzString::from(size))),
    );
    interactive(s, app, entry, row)
}

/// What the Size column says: a file's size once its stat is in, a folder's item count once it
/// is counted (one `read_dir`, no stat per item), else nothing yet.
fn size_text(s: &DriveState, entry: &Entry) -> String {
    if entry.is_folder {
        return s
            .counts
            .get(&entry.key)
            .map(|n| browse::counted(*n, "item", "items"))
            .unwrap_or_default();
    }
    model::cell_text(entry, Column::Size, s.settings.show_extensions)
}

/// A cell of the icon layouts: the big icon over the name, in a box of the layout's fixed size
/// (the virtual view places a line by its position).
fn icon_cell(s: &DriveState, app: &RefAny, entry: &Entry, layout: ViewLayout) -> Dom {
    let px = layout.icon_px();
    let width = layout.cell_width() + 8.0;
    let height = line_px(s, layout) - 6.0;
    let mut cell = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; align-items: center; box-sizing: border-box; \
         width: {width}px; height: {height}px; padding: 6px 4px; margin: 3px; flex-shrink: 0; \
         overflow: hidden; {}",
        cell_paint(s, entry)
    ));
    if s.settings.item_checkboxes {
        cell.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-self: flex-start;")
                .with_child(check_box(s, app, entry)),
        );
    }
    match s.thumbnails.get(&entry.key) {
        // A picture shows itself (Explorer's thumbnails).
        Some(Some(image)) => cell.add_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: row; align-items: center; \
                     justify-content: center; width: {px}px; height: {px}px; flex-shrink: 0;"
                ))
                .with_child(
                    Dom::create_image(image.clone())
                        .with_class(ids::THUMBNAIL_CLASS)
                        .with_css(format!("max-width: {px}px; max-height: {px}px;")),
                ),
        ),
        _ => cell.add_child(
            Dom::create_icon(AzString::from(icon_for(entry)))
                .with_css(format!("font-size: {px}px; flex-shrink: 0; {}", icon_colour(entry))),
        ),
    }
    if s.renaming.as_ref().is_some_and(|r| r.key == entry.key) {
        cell.add_child(rename_field(s, app));
    } else {
        cell.add_child(
            Dom::create_span_with_text(AzString::from(
                entry.display_name(s.settings.show_extensions),
            ))
            .with_class(ids::NAME_CLASS)
            .with_css(
                "font-size: 12px; text-align: center; max-width: 100%; overflow: hidden; \
                 max-height: 32px; margin-top: 4px;",
            ),
        );
    }
    interactive(s, app, entry, cell)
}

/// A cell of the List and Small icons layouts: a small icon beside the name.
fn inline_cell(s: &DriveState, app: &RefAny, entry: &Entry, layout: ViewLayout) -> Dom {
    let width = layout.cell_width() + 8.0;
    let mut cell = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; align-items: center; box-sizing: border-box; \
         width: {width}px; height: 24px; padding: 0px 4px; margin: 1px; border-radius: 4px; \
         flex-shrink: 0; {}",
        item_paint(s, entry)
    ));
    if s.settings.item_checkboxes {
        cell.add_child(check_box(s, app, entry));
    }
    cell.add_child(name_cell(s, app, entry, 18.0));
    interactive(s, app, entry, cell)
}

/// The box of a tile of the Tiles layout, its margin outside it.
const TILE_BOX: (f32, f32) = (250.0, 72.0);

/// A tile of the Tiles layout (the Tile widget: icon, name, type and size) in a box of the
/// layout's fixed size.
fn tile_cell(s: &DriveState, app: &RefAny, entry: &Entry) -> Dom {
    let (width, height) = TILE_BOX;
    let selected = s.selection.contains(&entry.key);
    let mut frame = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; align-items: center; box-sizing: border-box; \
         width: {width}px; height: {height}px; margin: 0px 8px 8px 0px; flex-shrink: 0; \
         overflow: hidden;{}",
        if actions::is_cut(s, &entry.key) {
            " opacity: 0.5;"
        } else {
            ""
        }
    ));
    if s.settings.item_checkboxes {
        frame.add_child(check_box(s, app, entry));
    }
    if s.renaming.as_ref().is_some_and(|r| r.key == entry.key) {
        frame.add_child(name_cell(s, app, entry, 32.0));
        return interactive(s, app, entry, frame);
    }
    let detail = if entry.is_folder {
        entry.kind()
    } else if entry.known {
        format!("{}, {}", entry.kind(), browse::format_size(entry.size))
    } else {
        entry.kind()
    };
    frame.add_child(
        Tile::create(AzString::from(
            entry.display_name(s.settings.show_extensions),
        ))
        .with_icon(AzString::from(icon_for(entry)))
        .with_detail(AzString::from(detail))
        .with_selected(selected)
        .dom()
        .with_css(format!(
            "flex-grow: 1; min-width: 0px; height: 100%; {}",
            if selected { look::TILE_SELECTED } else { "" }
        )),
    );
    interactive(s, app, entry, frame)
}

// ==== A folder: a virtual view of lines ====

/// The height (px) of one line of items in `layout`: every line of a layout is this high, so
/// the virtual view knows where any line is without laying out the ones before it.
pub(crate) fn line_px(s: &DriveState, layout: ViewLayout) -> f32 {
    match layout {
        ViewLayout::Details | ViewLayout::List | ViewLayout::SmallIcons => 26.0,
        ViewLayout::Content => 56.0,
        ViewLayout::Tiles => TILE_BOX.1 + 8.0,
        ViewLayout::ExtraLargeIcons | ViewLayout::LargeIcons | ViewLayout::MediumIcons => {
            let checks = if s.settings.item_checkboxes { 22.0 } else { 0.0 };
            layout.icon_px() + 62.0 + checks
        }
    }
}

/// How many items a line of `layout` holds in a view `width` px wide (one in Details and
/// Content): the cells' boxes with their margins, inside the line's padding.
pub(crate) fn columns_in(layout: ViewLayout, width: f32) -> usize {
    let cell = match layout {
        ViewLayout::Details | ViewLayout::Content => return 1,
        ViewLayout::Tiles => TILE_BOX.0 + 8.0,
        ViewLayout::List | ViewLayout::SmallIcons => layout.cell_width() + 10.0,
        ViewLayout::ExtraLargeIcons | ViewLayout::LargeIcons | ViewLayout::MediumIcons => {
            layout.cell_width() + 14.0
        }
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = ((width - 16.0) / cell).floor().max(1.0) as usize;
    n
}

/// The folder view as the virtual view draws it: the shown items in order (grouped: group
/// after group), the groups' headers, the lines and where each starts.
pub(crate) struct FolderModel<'a> {
    pub order: Vec<&'a Entry>,
    /// Grouped: every group's header and how many items it holds.
    pub groups: Vec<(String, usize)>,
    pub columns: usize,
    pub line_px: f32,
    pub lines: Vec<Line>,
    pub tops: Vec<f32>,
    pub total: f32,
}

/// [`FolderModel`] of the open folder in a view `width` px wide.
pub(crate) fn folder_model(s: &DriveState, width: f32) -> FolderModel<'_> {
    let layout = s.settings.layout;
    let visible = s.visible_entries();
    let grouped = s.settings.group_by != model::GroupBy::None;
    let (order, groups) = if grouped {
        let now = actions::now_secs() as i64;
        let mut order = Vec::with_capacity(visible.len());
        let mut groups = Vec::new();
        for g in model::group_entries(&visible, s.settings.group_by, now) {
            groups.push((g.label, g.entries.len()));
            order.extend(g.entries);
        }
        (order, groups)
    } else {
        (visible, Vec::new())
    };
    let (sizes, open): (Vec<usize>, Vec<bool>) = if grouped {
        groups
            .iter()
            .map(|(label, n)| (*n, !s.groups_closed.contains(label)))
            .unzip()
    } else {
        (vec![order.len()], vec![true])
    };
    let columns = columns_in(layout, width);
    let lines = listing::lines_of(&sizes, &open, columns, grouped);
    let line_px = line_px(s, layout);
    let (tops, total) = listing::line_tops(&lines, line_px);
    FolderModel {
        order,
        groups,
        columns,
        line_px,
        lines,
        tops,
        total,
    }
}

/// The width the folder view draws its lines in: as it last drew (the virtual view records
/// it), else the content's estimate - for the work done for the rows in view (their stats,
/// counts and thumbnails) and for the line a key reveals.
fn view_width(s: &DriveState) -> f32 {
    if s.view_width > 0.0 {
        s.view_width
    } else {
        s.content_estimate().0
    }
}

/// The folder view's height: as it last drew or scrolled, else the content's estimate.
fn view_height(s: &DriveState) -> f32 {
    if s.view_scroll.1 > 0.0 {
        s.view_scroll.1
    } else {
        s.content_estimate().1
    }
}

/// The positions (in the shown order) of the items in view now, by the view's last scroll -
/// the IconGrid's top row in its layouts.
pub(crate) fn items_in_view(s: &DriveState) -> Range<usize> {
    if uses_icon_grid(s) {
        let (width, height) = s.content_estimate();
        let layout = s.settings.layout;
        let columns = ((width / layout.cell_width()).floor().max(1.0)) as usize;
        let rows = ((height / (layout.icon_px() + 40.0)).ceil() as usize)
            .max(1)
            .saturating_add(1);
        let start = s.grid_view.top_row.saturating_mul(columns);
        return start..start.saturating_add(rows.saturating_mul(columns));
    }
    let model = folder_model(s, view_width(s));
    let lines =
        listing::lines_in_view(&model.tops, model.total, s.view_scroll.0, view_height(s));
    listing::positions_of(&model.lines, lines)
}

/// The items of the folder view in its shown order (grouped: group after group), as
/// [`items_in_view`] counts positions.
pub(crate) fn shown_order(s: &DriveState) -> Vec<&Entry> {
    if s.settings.group_by == model::GroupBy::None {
        return s.visible_entries();
    }
    let visible = s.visible_entries();
    let now = actions::now_secs() as i64;
    model::group_entries(&visible, s.settings.group_by, now)
        .into_iter()
        .flat_map(|g| g.entries)
        .collect()
}

/// What the folder's virtual view carries: the app (the rows are read from its state when the
/// view draws, so a scroll needs no rebuild of the window).
struct FolderRowsRef {
    app: RefAny,
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::create(LogicalPosition::create(x, y), LogicalSize::create(w, h))
}

/// The folder's virtual view: the lines in view and a screen either side, one under the other
/// from the first one's place; the document is every line, however many items the folder has.
extern "C" fn folder_lines(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let Some(rows) = data.downcast_ref::<FolderRowsRef>() else {
        return VirtualViewReturn::default();
    };
    let app = rows.app.clone();
    let mut state = app.clone();
    let Some(mut guard) = state.downcast_mut::<DriveState>() else {
        return VirtualViewReturn::default();
    };
    let size = info.bounds.get_logical_size();
    // The box the view draws in is what the work for the rows in view counts with (their
    // stats, counts and thumbnails; the line a key reveals): the window's estimate errs by
    // the frames around the view, a column's worth in the grids.
    guard.view_width = size.width;
    if size.height > 0.0 {
        guard.view_scroll.1 = size.height;
    }
    let s = &*guard;
    let layout = s.settings.layout;
    // A Details view is as wide as its columns (it scrolls across in the view's host).
    let width = if layout == ViewLayout::Details {
        size.width.max(details_width(s))
    } else {
        size.width.max(1.0)
    };
    let model = folder_model(s, width);
    let window = listing::lines_window(&model.tops, model.total, info.scroll_offset.y, size.height);
    let mut root = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; width: {width}px;"
    ));
    for i in window.clone() {
        root.add_child(line_dom(s, &app, &model, i));
    }
    let top = model.tops.get(window.start).copied().unwrap_or(0.0);
    let bottom = model
        .tops
        .get(window.end)
        .copied()
        .unwrap_or(model.total);
    VirtualViewReturn::with_dom(
        root,
        rect(0.0, top, width, (bottom - top).max(1.0)),
        rect(0.0, 0.0, width, model.total.max(1.0)),
    )
}

/// The Details columns' width, the check boxes' column with them.
fn details_width(s: &DriveState) -> f32 {
    let checks = if s.settings.item_checkboxes { 28.0 } else { 0.0 };
    s.settings.columns.columns.iter().map(|c| c.width + 8.0).sum::<f32>() + checks
}

/// Line `index` of the view: a group's header, or its items (a Details or Content row, or a row
/// of cells).
fn line_dom(s: &DriveState, app: &RefAny, model: &FolderModel<'_>, index: usize) -> Dom {
    match model.lines[index] {
        Line::Header { group } => {
            let (label, count) = model.groups.get(group).cloned().unwrap_or_default();
            group_header(s, app, &label, count)
        }
        Line::Items { start, end } => {
            let layout = s.settings.layout;
            let items = &model.order[start..end];
            match layout {
                ViewLayout::Details => details_row(s, app, items[0], start % 2 == 1),
                ViewLayout::Content => content_row(s, app, items[0]),
                _ => {
                    let cells: Vec<Dom> = items
                        .iter()
                        .map(|entry| match layout {
                            ViewLayout::Tiles => tile_cell(s, app, entry),
                            ViewLayout::List | ViewLayout::SmallIcons => {
                                inline_cell(s, app, entry, layout)
                            }
                            _ => icon_cell(s, app, entry, layout),
                        })
                        .collect();
                    Dom::create_div()
                        .with_css(format!(
                            "display: flex; flex-direction: row; align-items: flex-start; \
                             box-sizing: border-box; height: {}px; padding: 0px 8px; \
                             flex-shrink: 0; overflow: hidden;",
                            model.line_px
                        ))
                        .with_children(DomVec::from(cells))
                }
            }
        }
    }
}

/// What a group header carries: the app and the group's label (a click opens or closes it).
struct GroupHeaderRef {
    app: RefAny,
    label: String,
}

/// Explorer's group header: the triangle, the label, the count and a rule.
fn group_header(s: &DriveState, app: &RefAny, label: &str, count: usize) -> Dom {
    let open = !s.groups_closed.contains(label);
    Dom::create_div()
        .with_class(ids::GROUP_HEADER_CLASS)
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; box-sizing: border-box; \
             height: {}px; padding: 0px 8px; flex-shrink: 0; font-size: 13px; font-weight: 600; \
             cursor: default; {}",
            listing::HEADER_PX,
            look::ITEM
        ))
        .with_accessibility_name(label)
        .with_child(
            Dom::create_icon(AzString::from(if open {
                "expand_more"
            } else {
                "chevron_right"
            }))
            .with_css("font-size: 16px; margin-right: 4px; opacity: 0.7; flex-shrink: 0;"),
        )
        .with_child(Dom::create_span_with_text(AzString::from(label)))
        .with_child(
            Dom::create_span_with_text(AzString::from(format!(" ({count})")))
                .with_css("opacity: 0.6; font-weight: 400; margin-right: 8px;"),
        )
        .with_child(Dom::create_div().with_css(
            "flex-grow: 1; height: 1px; opacity: 0.35; background: currentcolor;",
        ))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(GroupHeaderRef {
                app: app.clone(),
                label: label.to_string(),
            }),
            on_group_header_click,
        )
}

/// A click on a group's header opens or closes the group.
extern "C" fn on_group_header_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, label)) = data
        .downcast_ref::<GroupHeaderRef>()
        .map(|g| (g.app.clone(), g.label.clone()))
    else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    with_state(&mut app, &mut info, |info, app, s| {
        if !s.groups_closed.remove(&label) {
            s.groups_closed.insert(label);
        }
        actions::request_view_work(info, app, s);
    })
}

/// The view's scroll settled: where it is now decides which rows get their stats, counts and
/// thumbnails (the rows already drawn follow the scroll by themselves).
extern "C" fn on_view_scrolled(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut app) = data
        .downcast_ref::<FolderRowsRef>()
        .map(|r| r.app.clone())
    else {
        return Update::DoNothing;
    };
    let hit = info.get_hit_node();
    let Some(raw) = hit.node.into_raw().checked_sub(1) else {
        return Update::DoNothing;
    };
    let offset = info
        .get_scroll_offset_for_node(hit.dom, NodeId { inner: raw })
        .into_option()
        .map_or(0.0, |p| p.y);
    let height = info.get_node_size(hit).into_option().map_or(0.0, |size| size.height);
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.view_scroll = (offset, height);
    // The rows in view already show; what they lack (sizes, counts, thumbnails) is fetched on a
    // worker thread and rebuilds the window when it is in - a scroll itself rebuilds nothing.
    actions::request_view_work(&mut info, &handle, s);
    Update::DoNothing
}

/// Scrolls the folder's rows back to their top (a new folder opens at its top): the view node,
/// found by its id, keeps its identity across the rebuild, and with it the scroll offset.
pub(crate) fn scroll_view_to_top(info: &mut CallbackInfo) {
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, ids::FOLDER_ROWS);
    if node.into_raw() == 0 {
        return;
    }
    info.scroll_to(dom, node, LogicalPosition::create(0.0, 0.0));
}

/// Scrolls the folder's rows so the item `key` is in view (the arrow keys, a new folder waiting
/// for its name).
pub(crate) fn reveal_item(info: &mut CallbackInfo, s: &mut DriveState, key: &str) {
    if uses_icon_grid(s) {
        return; // the grid keeps its focused item in view itself
    }
    let model = folder_model(s, view_width(s));
    let Some(position) = model.order.iter().position(|e| e.key == key) else {
        return;
    };
    let Some(line) = model.lines.iter().position(
        |l| matches!(*l, Line::Items { start, end } if start <= position && position < end),
    ) else {
        return;
    };
    let top = model.tops[line];
    let bottom = top + model.line_px;
    let scroll = s.view_scroll.0;
    let height = view_height(s);
    let y = if top < scroll {
        top
    } else if bottom > scroll + height {
        bottom - height
    } else {
        return;
    };
    let dom = DomId { inner: 0 };
    let node = info.get_node_id_by_id_attribute(dom, ids::FOLDER_ROWS);
    if node.into_raw() == 0 {
        return;
    }
    s.view_scroll.0 = y.max(0.0);
    info.scroll_to(dom, node, LogicalPosition::create(0.0, y.max(0.0)));
}

/// The open folder in its layout, grouped or not: the Details header over a virtual view of
/// the lines (only the lines in view and a screen either side are ever built).
fn folder_view(s: &DriveState, app: &RefAny) -> Dom {
    if s.visible_entries().is_empty() {
        if s.loading || !s.listing_done {
            return Dom::create_div()
                .with_css("padding: 16px; opacity: 0.7;")
                .with_child(Dom::create_span_with_text(AzString::from("Loading...")));
        }
        if s.listing_failed {
            return ShellEmptyState::create(AzString::from("This folder could not be read."))
                .with_icon(AzString::from("folder_off"))
                .with_detail(AzString::from(
                    "The message above says why; Refresh (F5) tries again.",
                ))
                .dom()
                .with_id(ids::EMPTY_FOLDER);
        }
        let (title, detail) = if s.search.trim().is_empty() {
            (
                "This folder is empty.",
                "Drop files here from your computer, or use New folder (or Upload) on the \
                 ribbon's Home tab.",
            )
        } else {
            (
                "No items match your search.",
                "The search looks at the names in this folder.",
            )
        };
        return ShellEmptyState::create(AzString::from(title))
            .with_icon(AzString::from("folder_open"))
            .with_detail(AzString::from(detail))
            .dom()
            .with_id(ids::EMPTY_FOLDER);
    }
    let layout = s.settings.layout;
    let mut view = Dom::create_div()
        .with_id(ids::FOLDER_VIEW)
        .with_class(ids::layout_class(layout))
        .with_css(if layout == ViewLayout::Details {
            format!(
                "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                 min-width: {}px;",
                details_width(s)
            )
        } else {
            String::from("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        });
    if layout == ViewLayout::Details {
        view.add_child(details_header(s, app));
    }
    let rows = RefAny::new(FolderRowsRef { app: app.clone() });
    view.add_child(
        Dom::create_virtual_view(rows.clone(), folder_lines)
            .with_id(ids::FOLDER_ROWS)
            .with_accessibility_name(s.place_name().as_str())
            .with_css("flex-grow: 1; min-height: 0px; width: 100%;")
            .with_callback(
                EventFilter::Hover(HoverEventFilter::ScrollEnd),
                rows,
                on_view_scrolled,
            ),
    );
    view
}

// ==== The icon layouts: azul's IconGrid ====

/// The grid's data: the app and the positions (in `entries`) of the items it shows, in order -
/// an item is made when the grid asks for it (only the ones in view), never one per item of
/// the folder per rebuild.
struct GridItems {
    app: RefAny,
    order: Vec<usize>,
}

/// The grid's data callback: item `index` of the visible order.
extern "C" fn grid_item(mut data: RefAny, index: usize) -> IconGridItem {
    let Some(grid) = data.downcast_ref::<GridItems>() else {
        return IconGridItem::empty();
    };
    let Some(&at) = grid.order.get(index) else {
        return IconGridItem::empty();
    };
    let mut app = grid.app.clone();
    let Some(s) = app.downcast_ref::<DriveState>() else {
        return IconGridItem::empty();
    };
    let Some(entry) = s.entries.get(at) else {
        return IconGridItem::empty();
    };
    let mut item = IconGridItem::create(
        entry.display_name(s.settings.show_extensions),
        icon_for(entry),
    )
    .with_name(format!("{}, {}", entry.name, entry.kind()));
    if let Some(Some(image)) = s.thumbnails.get(&entry.key) {
        item = item.with_image(image.clone());
    }
    if in_the_cloud(&s, entry) {
        item = item.with_badge("cloud_queue");
    }
    item
}

/// Extra large, Large and Medium icons: the folder's items as azul's IconGrid at the content
/// pane's size - a picture's thumbnail once it is made, a cloud badge on a cloud drive's files
/// (fetched when opened), Explorer's pointer (click, Ctrl / Shift, the rubber band, a drag
/// out) and keyboard. Its selection is the app's (by position in the visible order).
fn icon_grid(s: &DriveState, app: &RefAny, size: (f32, f32)) -> Dom {
    let layout = s.settings.layout;
    let show_hidden = s.settings.show_hidden;
    let order: Vec<usize> = s
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| show_hidden || !e.is_hidden())
        .filter(|(_, e)| browse::matches_search(e, &s.search))
        .map(|(i, _)| i)
        .collect();
    let keys: Vec<&str> = order.iter().map(|&i| s.entries[i].key.as_str()).collect();
    let count = order.len();
    let icon = layout.icon_px();
    let view = s.grid_view.clone().with_selection(s.selection.positions(&keys));
    Dom::create_div()
        .with_id(ids::FOLDER_VIEW)
        .with_class(ids::layout_class(layout))
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(
            IconGrid::create(count, size.0, size.1)
                .with_id(ids::ICON_GRID)
                .with_accessibility_name(s.place_name())
                .with_cell_size(layout.cell_width(), icon + 40.0, icon)
                .with_view(view)
                .with_data_source(
                    RefAny::new(GridItems {
                        app: app.clone(),
                        order,
                    }),
                    grid_item as IconGridDataSourceCallbackType,
                )
                .with_on_event(app.clone(), on_grid_event as IconGridOnEventCallbackType)
                .dom(),
        )
}

/// What the grid did. Its view is kept (the first row, a rubber band); its selection is the
/// app's from now on (the preview follows it); a double-click or Enter opens, a right click
/// shows the selection's menu (the folder's on empty space), a drag carries the selection; a
/// scroll brings the thumbnails of the pictures that came into view.
extern "C" fn on_grid_event(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: IconGridEvent,
) -> Update {
    let kind = event.kind;
    let index = event.index.into_option();
    let view = event.view;
    with_state(&mut data, &mut info, |info, app, s| {
        let keys = s.visible_keys();
        let order: Vec<&str> = keys.iter().map(String::as_str).collect();
        let before = s.selection.keys();
        s.selection.adopt(&order, &view.selection);
        s.grid_view = view;
        if s.selection.keys() != before {
            s.type_ahead.clear();
            s.print_selection();
            actions::request_preview(info, app, s);
        }
        match kind {
            IconGridEventKind::Activate => {
                if let Some(key) = index.and_then(|i| keys.get(i)).cloned() {
                    actions::activate(info, app, s, &key);
                }
            }
            IconGridEventKind::ContextMenu => {
                let menu = actions::context_menu(app, s);
                info.open_menu(menu);
            }
            IconGridEventKind::DragStart => {
                if let Some(drive) = s.current_drive_id() {
                    let items = s.selected_items();
                    s.dragging = Some((drive, items));
                }
            }
            IconGridEventKind::Scroll => {
                actions::request_view_work(info, app, s);
            }
            IconGridEventKind::Select | IconGridEventKind::Drag => {}
        }
    })
}
