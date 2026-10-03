//! The content pane: This PC (the drives as tiles in groups), Quick access
//! (the pinned folders and the recent places), or a folder in one of
//! Explorer's eight layouts - grouped or not, with item check boxes or not -
//! with the InfoBar over it. Every item takes a click (Ctrl / Shift too), a
//! double-click, a right-click (the context menu), a drag (a folder takes
//! the drop: a move, Ctrl a copy) and F2's rename field.

use azul::{
    callbacks::{
        AccordionOnToggleCallbackType, ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType,
        TextInputOnFocusLostCallbackType, TextInputOnTextInputCallbackType,
        TextInputOnVirtualKeyDownCallbackType, TileOnClickCallbackType,
    },
    dom::{AttributeType, FocusTarget, VirtualKeyCode},
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{
        Accordion, AccordionSection, AccordionVariant, CheckBoxState, InfoBar, OnTextInputReturn,
        TextInputState, TextInputValid, Tile, TileCapacity,
    },
};
use crate::{
    actions::{self, action_ref, on_action, Action},
    browse::{self, Column, Entry, Place},
    go, ids,
    model::{self, ViewLayout},
    preview::{self, PreviewKind},
    save_settings, with_state, ColumnDrag, DriveState, Message,
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

/// The colour of an item's icon: Explorer's folder yellow, else the ink.
fn icon_colour(entry: &Entry) -> &'static str {
    if entry.is_folder {
        "color: #D9A93B;"
    } else {
        ""
    }
}

/// The paint of an item: selected, focused, cut (faded), hovered.
fn item_paint(s: &DriveState, entry: &Entry) -> String {
    let mut css = String::from(
        "border: 1px solid transparent; border-radius: 3px; cursor: default; \
         :hover { background: var(--az-accent-glow, rgba(0, 120, 215, 0.10)); }",
    );
    if s.selection.contains(&entry.key) {
        css.push_str(" background: var(--az-accent-soft, rgba(0, 120, 215, 0.22));");
    }
    if s.selection.focus() == Some(entry.key.as_str()) {
        css.push_str(" border: 1px solid var(--az-accent, #2F4A85);");
    }
    if actions::is_cut(s, &entry.key) {
        css.push_str(" opacity: 0.5;");
    }
    css
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

/// The content pane.
pub(crate) fn content(s: &DriveState, app: &RefAny) -> Dom {
    let mut area = Dom::create_div().with_id(ids::CONTENT).with_css(
        "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px;",
    );
    if let Some(message) = &s.message {
        area.add_child(info_bar(message, app));
    }
    let view = match &s.place {
        Place::ThisPc => this_pc(s, app),
        Place::QuickAccess => quick_access(s, app),
        Place::Folder { .. } => folder_view(s, app),
    };
    let background = RefAny::new(BackgroundRef { app: app.clone() });
    area.add_child(
        Dom::create_div()
            .with_id(ids::VIEW)
            .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow: auto;")
            .with_child(view)
            // A click on the empty space selects nothing; a right-click there
            // is the folder's menu.
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                background.clone(),
                on_background_click,
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::RightMouseUp),
                background.clone(),
                on_background_menu,
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Drop),
                background,
                on_background_drop,
            ),
    );
    if s.next.is_some() && !s.loading && s.current_drive().is_some() {
        area.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; padding: 6px 12px;")
                .with_child(
                    Button::create(AzString::from("Load more"))
                        .with_on_click(app.clone(), on_load_more as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::LOAD_MORE),
                ),
        );
    }
    area
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

extern "C" fn on_load_more(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if !s.loading {
            crate::start_listing(info, app, s, true);
        }
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
        .with_css("width: 260px; margin: 0px 8px 8px 0px;")
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
        String::from("Cloud drives (S3)"),
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
                .with_css("width: 250px; margin: 0px 8px 8px 0px;")
        })
        .collect();
    let pinned = if pins.is_empty() {
        ShellEmptyState::create(AzString::from("Nothing is pinned yet."))
            .with_icon(AzString::from("push_pin"))
            .with_detail(AzString::from(
                "Open a folder and choose Pin to Quick access on the HOME tab.",
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
                .with_css("width: 250px; margin: 0px 8px 8px 0px;")
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
    _state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let update = match key {
        Some(VirtualKeyCode::Return) | Some(VirtualKeyCode::NumpadEnter) => {
            info.stop_propagation();
            with_state(&mut data, &mut info, |info, app, s| {
                actions::commit_rename(info, app, s)
            })
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

extern "C" fn on_rename_blur(mut data: RefAny, mut info: CallbackInfo, _state: TextInputState) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
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
    let label = if s.renaming.as_ref().is_some_and(|r| r.key == entry.key) {
        rename_field(s, app)
    } else {
        Dom::create_span_with_text(AzString::from(
            entry.display_name(s.settings.show_extensions),
        ))
        .with_class(ids::NAME_CLASS)
        .with_css("overflow: hidden; text-overflow: ellipsis; white-space: nowrap;")
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; min-width: 0px;")
        .with_child(icon)
        .with_child(label)
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

/// The Details header: the columns (a click sorts, again reverses), each
/// with an edge to drag (its width) or double-click (to fit).
fn details_header(s: &DriveState, app: &RefAny) -> Dom {
    let mut row = Dom::create_div().with_id(ids::DETAILS_HEADER).with_css(
        "display: flex; flex-direction: row; font-size: 12px; \
         border-bottom: 1px solid rgba(128, 128, 128, 0.35);",
    );
    if s.settings.item_checkboxes {
        row.add_child(Dom::create_div().with_css("width: 28px; flex-shrink: 0;"));
    }
    for c in &s.settings.columns.columns {
        let width = c.width;
        let data = RefAny::new(ColumnRef {
            app: app.clone(),
            column: c.column,
        });
        let mut cell = Dom::create_div()
            .with_class(ids::COLUMN_CLASS)
            .with_css(format!(
                "display: flex; flex-direction: row; align-items: center; width: {width}px; \
                 min-width: {width}px; flex-shrink: 0; padding: 4px 0px 4px 8px; \
                 :hover {{ background: var(--az-accent-glow, rgba(0, 120, 215, 0.10)); }}"
            ))
            .with_child(
                Dom::create_span_with_text(AzString::from(c.column.label()))
                    .with_css("flex-grow: 1; overflow: hidden; white-space: nowrap;"),
            );
        if s.settings.sort.column == c.column {
            let arrow = if s.settings.sort.descending {
                "arrow_drop_down"
            } else {
                "arrow_drop_up"
            };
            cell.add_child(Dom::create_icon(AzString::from(arrow)).with_css("font-size: 16px;"));
        }
        cell.add_child(
            Dom::create_div()
                .with_class(ids::COLUMN_EDGE_CLASS)
                .with_css(
                    "width: 6px; align-self: stretch; flex-shrink: 0; cursor: col-resize; \
                     border-right: 1px solid rgba(128, 128, 128, 0.35);",
                )
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

/// A row of the Details layout.
fn details_row(s: &DriveState, app: &RefAny, entry: &Entry) -> Dom {
    let mut row = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; align-items: center; height: 26px; {}",
        item_paint(s, entry)
    ));
    if s.settings.item_checkboxes {
        row.add_child(check_box(s, app, entry));
    }
    for c in &s.settings.columns.columns {
        let width = c.width;
        let (cell, extra) = match c.column {
            Column::Name => (name_cell(s, app, entry, 18.0), ""),
            Column::Size => (
                Dom::create_span_with_text(AzString::from(model::cell_text(
                    entry,
                    c.column,
                    s.settings.show_extensions,
                ))),
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
        "display: flex; flex-direction: row; align-items: center; height: 54px; padding: 0px 8px; \
         border-bottom: 1px solid rgba(128, 128, 128, 0.18); {}",
        item_paint(s, entry)
    ));
    if s.settings.item_checkboxes {
        row.add_child(check_box(s, app, entry));
    }
    let size = if entry.is_folder {
        String::new()
    } else {
        format!("Size: {}", browse::format_size(entry.size))
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

/// A cell of the icon layouts: the big icon over the name.
fn icon_cell(s: &DriveState, app: &RefAny, entry: &Entry, layout: ViewLayout) -> Dom {
    let px = layout.icon_px();
    let width = layout.cell_width();
    let mut cell = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: column; align-items: center; width: {width}px; \
         padding: 6px 4px; margin: 2px; {}",
        item_paint(s, entry)
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
                     justify-content: center; width: {px}px; height: {px}px;"
                ))
                .with_child(
                    Dom::create_image(image.clone())
                        .with_class(ids::THUMBNAIL_CLASS)
                        .with_css(format!("max-width: {px}px; max-height: {px}px;")),
                ),
        ),
        _ => cell.add_child(
            Dom::create_icon(AzString::from(icon_for(entry)))
                .with_css(format!("font-size: {px}px; {}", icon_colour(entry))),
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
    let width = layout.cell_width();
    let mut cell = Dom::create_div().with_css(format!(
        "display: flex; flex-direction: row; align-items: center; width: {width}px; \
         height: 24px; padding: 0px 4px; margin: 1px; {}",
        item_paint(s, entry)
    ));
    if s.settings.item_checkboxes {
        cell.add_child(check_box(s, app, entry));
    }
    cell.add_child(name_cell(s, app, entry, 18.0));
    interactive(s, app, entry, cell)
}

/// A tile of the Tiles layout (the Tile widget: icon, name, type and size).
fn tile_cell(s: &DriveState, app: &RefAny, entry: &Entry) -> Dom {
    if s.renaming.as_ref().is_some_and(|r| r.key == entry.key) {
        return inline_cell(s, app, entry, ViewLayout::Tiles);
    }
    let detail = if entry.is_folder {
        entry.kind()
    } else {
        format!("{}, {}", entry.kind(), browse::format_size(entry.size))
    };
    let mut css = String::from("width: 250px; margin: 0px 8px 8px 0px;");
    if actions::is_cut(s, &entry.key) {
        css.push_str(" opacity: 0.5;");
    }
    let tile = Tile::create(AzString::from(
        entry.display_name(s.settings.show_extensions),
    ))
    .with_icon(AzString::from(icon_for(entry)))
    .with_detail(AzString::from(detail))
    .with_selected(s.selection.contains(&entry.key))
    .dom()
    .with_css(css);
    if s.settings.item_checkboxes {
        let row = Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center;")
            .with_child(check_box(s, app, entry))
            .with_child(tile);
        return interactive(s, app, entry, row);
    }
    interactive(s, app, entry, tile)
}

/// The items of one group in the layout.
fn items_of(s: &DriveState, app: &RefAny, layout: ViewLayout, entries: &[&Entry]) -> Dom {
    match layout {
        ViewLayout::Details => Dom::create_div()
            .with_css("display: flex; flex-direction: column;")
            .with_children(DomVec::from(
                entries
                    .iter()
                    .map(|e| details_row(s, app, e))
                    .collect::<Vec<_>>(),
            )),
        ViewLayout::Content => Dom::create_div()
            .with_css("display: flex; flex-direction: column;")
            .with_children(DomVec::from(
                entries
                    .iter()
                    .map(|e| content_row(s, app, e))
                    .collect::<Vec<_>>(),
            )),
        ViewLayout::Tiles => wrap_row(entries.iter().map(|e| tile_cell(s, app, e)).collect()),
        ViewLayout::List | ViewLayout::SmallIcons => {
            wrap_row(entries.iter().map(|e| inline_cell(s, app, e, layout)).collect())
        }
        ViewLayout::ExtraLargeIcons | ViewLayout::LargeIcons | ViewLayout::MediumIcons => {
            wrap_row(entries.iter().map(|e| icon_cell(s, app, e, layout)).collect())
        }
    }
}

/// The open folder in its layout, grouped or not.
fn folder_view(s: &DriveState, app: &RefAny) -> Dom {
    let visible = s.visible_entries();
    if visible.is_empty() {
        if s.loading {
            return Dom::create_div()
                .with_css("padding: 16px; opacity: 0.7;")
                .with_child(Dom::create_span_with_text(AzString::from("Loading...")));
        }
        let (title, detail) = if s.search.trim().is_empty() {
            (
                "This folder is empty.",
                "Drop files here from your computer, or use New folder or Upload on the HOME tab.",
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
    let now = actions::now_secs() as i64;
    let grouped = model::group_entries(&visible, s.settings.group_by, now);
    let mut sections: Vec<(String, usize, Dom)> = grouped
        .into_iter()
        .map(|g| {
            let count = g.entries.len();
            let body = items_of(s, app, layout, &g.entries);
            (g.label, count, body)
        })
        .collect();
    let body = if s.settings.group_by == model::GroupBy::None && sections.len() == 1 {
        sections.remove(0).2
    } else {
        groups(s, app, sections)
    };
    let mut view = Dom::create_div()
        .with_id(ids::FOLDER_VIEW)
        .with_class(ids::layout_class(layout))
        .with_css("display: flex; flex-direction: column;");
    if layout == ViewLayout::Details {
        view.add_child(details_header(s, app));
    }
    view.add_child(body);
    view
}
