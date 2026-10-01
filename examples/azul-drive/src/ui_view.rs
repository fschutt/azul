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
use azul_storage::config::DriveLocation;

use crate::{
    actions::{self, action_ref, on_action, Action},
    browse::{self, Column, Entry, Place},
    go,
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
        .with_id("info-bar")
}

extern "C" fn on_dismiss(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| s.message = None)
}

/// The content pane.
pub(crate) fn content(s: &DriveState, app: &RefAny) -> Dom {
    let mut area = Dom::create_div().with_id("content").with_css(
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
            .with_id("view")
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
                        .with_id("load-more"),
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
            s.preview = None;
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
    let copy = info.get_key_modifiers().ctrl || info.get_key_modifiers().meta;
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
                "{}, {count} items at the root",
                slot.kind()
            )));
        }
        (None, None) => tile = tile.with_detail(AzString::from(slot.kind())),
    }
    let place = Place::folder(&slot.entry.id, "");
    let data = place_ref(app, Some(index), None, place);
    tile.with_on_click(data.clone(), on_place_click as TileOnClickCallbackType)
        .with_on_double_click(data, on_place_open as TileOnClickCallbackType)
        .dom()
        .with_class(AzString::from("azdrive-drive"))
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
    groups(s, app, sections).with_id("this-pc")
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
    .with_id("quick-access")
}
