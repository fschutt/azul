//! The panes around the content: the address bar, the navigation pane
//! (Quick access, This PC), the preview pane, the details pane and the
//! status bar.

use azul::{
    audio::{AudioConfig, AudioFrame, AudioSink},
    callbacks::{
        AddressBarOnEventCallbackType, ButtonOnClickCallbackType,
        ShellNavigationPaneOnEventCallbackType, StatusBarOnViewSelectCallbackType,
    },
    image::RawImageFormat,
    menu::{Menu, MenuItem},
    prelude::*,
    shells::{
        ShellNavigationGroup, ShellNavigationPane, ShellNavigationPaneEvent,
        ShellNavigationPaneEventKind,
    },
    str::String as AzString,
    vec::{F32Vec, StatusBarSegmentVec, StatusBarViewVec, StringVec},
    video::{VideoConfig, VideoSource},
    widgets::{
        AddressBar, AddressBarEvent, AddressBarEventKind, DetailsPane, StatusBar,
        StatusBarSegment, StatusBarView, StatusBarViewSwitcher, TreeViewNode, VideoWidget,
    },
};
use azul_storage::{config::DriveLocation, key};

use crate::{
    actions::{self, action_ref, menu_item, on_action, Action},
    browse::{self, Place},
    go,
    jobs::PreviewContent,
    model::ViewLayout,
    place_up, start_tree_listing, ui_view, with_state, DriveState,
};

// ==== The address bar ====

pub(crate) fn address_bar(s: &DriveState, app: &RefAny) -> Dom {
    let drive_name = s.drive_name(&s.place);
    let labels: Vec<AzString> = browse::crumbs_of(&s.place, &drive_name)
        .into_iter()
        .map(|(label, _)| AzString::from(label))
        .collect();
    let path_drive = match &s.place {
        Place::Folder { .. } => Some(drive_name.as_str()),
        _ => None,
    };
    AddressBar::create(StringVec::from(labels))
        .with_path(AzString::from(browse::path_text(&s.place, path_drive)))
        .with_search(AzString::from(s.search.as_str()))
        .with_search_placeholder(AzString::from(format!("Search {}", s.place_name())))
        .with_can_go(
            s.history.can_go_back(),
            s.history.can_go_forward(),
            place_up(&s.place).is_some(),
        )
        .with_editing(s.editing_path)
        .with_recent(true)
        .with_on_event(app.clone(), on_address as AddressBarOnEventCallbackType)
        .dom()
}

/// The folders under `place` the app knows: the tree's listing, or the
/// open folder's rows. `None` when it was never listed.
fn known_folders(s: &DriveState, place: &Place) -> Option<Vec<(String, Place)>> {
    match place {
        Place::QuickAccess => Some(
            s.settings
                .pinned
                .iter()
                .map(|p| (p.name.clone(), Place::folder(&p.drive, &p.prefix)))
                .collect(),
        ),
        Place::ThisPc => Some(
            s.slots
                .iter()
                .map(|slot| (slot.entry.name.clone(), Place::folder(&slot.entry.id, "")))
                .collect(),
        ),
        Place::Folder { drive, prefix } => {
            if let Some(folders) = s.tree.loaded.get(&(drive.clone(), prefix.clone())) {
                return Some(
                    folders
                        .iter()
                        .map(|f| (key::last_segment(f).to_string(), Place::folder(drive, f)))
                        .collect(),
                );
            }
            (*place == s.place && !s.loading).then(|| {
                s.entries
                    .iter()
                    .filter(|e| e.is_folder && (s.settings.show_hidden || !e.is_hidden()))
                    .map(|e| (e.name.clone(), Place::folder(drive, &e.key)))
                    .collect()
            })
        }
    }
}

extern "C" fn on_address(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: AddressBarEvent,
) -> Update {
    let index = event.index;
    let text = event.text.as_str().to_string();
    with_state(&mut data, &mut info, |info, app, s| match event.kind {
        AddressBarEventKind::Back => actions::run_command(info, app, s, crate::keys::Command::Back),
        AddressBarEventKind::Forward => {
            actions::run_command(info, app, s, crate::keys::Command::Forward)
        }
        AddressBarEventKind::Up => actions::run_command(info, app, s, crate::keys::Command::Up),
        AddressBarEventKind::Refresh => crate::refresh(info, app, s),
        AddressBarEventKind::Recent => actions::run_action(info, app, s, Action::RecentMenu),
        AddressBarEventKind::Crumb => {
            let drive_name = s.drive_name(&s.place);
            if let Some((_, place)) = browse::crumbs_of(&s.place, &drive_name).get(index) {
                let place = place.clone();
                go(info, app, s, place, true);
            }
        }
        AddressBarEventKind::CrumbMenu => {
            let drive_name = s.drive_name(&s.place);
            let Some((_, place)) = browse::crumbs_of(&s.place, &drive_name).get(index).cloned()
            else {
                return;
            };
            match known_folders(s, &place) {
                Some(folders) if !folders.is_empty() => {
                    let items: Vec<MenuItem> = folders
                        .into_iter()
                        .map(|(label, place)| menu_item(app, &label, Action::Go(place), false))
                        .collect();
                    info.open_menu_for_hit_node(Menu::create(items));
                }
                Some(_) => s.info("This folder has no subfolders."),
                None => {
                    if let Place::Folder { drive, prefix } = place {
                        s.info("Listing the folder...");
                        start_tree_listing(info, app, s, (drive, prefix));
                    }
                }
            }
        }
        AddressBarEventKind::EditStarted => s.editing_path = true,
        AddressBarEventKind::PathEntered => {
            s.editing_path = false;
            match browse::parse_path(&text, &s.drive_names()) {
                Some(place) => go(info, app, s, place, true),
                None => s.error(format!("There is no drive for \"{}\".", text.trim())),
            }
        }
        AddressBarEventKind::EditCancelled => s.editing_path = false,
        AddressBarEventKind::Search => {
            s.search = text.clone();
            let keys = s.visible_keys();
            let order: Vec<&str> = keys.iter().map(String::as_str).collect();
            s.selection.retain(&order);
        }
    })
}

// ==== The navigation pane ====

/// The places of a navigation group's tree, in its depth-first order.
fn group_places(s: &DriveState, group: usize) -> Vec<Place> {
    let mut places = Vec::new();
    match group {
        0 => {
            places.push(Place::QuickAccess);
            for pin in &s.settings.pinned {
                places.push(Place::folder(&pin.drive, &pin.prefix));
            }
        }
        _ => {
            let _ = this_pc_tree(s, &mut places);
        }
    }
    places
}

/// Quick access: the place itself, then the pinned folders.
fn quick_access_tree(s: &DriveState) -> TreeViewNode {
    let mut root = TreeViewNode::create(AzString::from(browse::QUICK_ACCESS))
        .with_icon(AzString::from("star"))
        .with_expanded(s.tree.quick_open)
        .with_selected(s.place == Place::QuickAccess);
    for pin in &s.settings.pinned {
        let place = Place::folder(&pin.drive, &pin.prefix);
        root = root.with_child(
            TreeViewNode::create(AzString::from(pin.name.as_str()))
                .with_icon(AzString::from("folder_special"))
                .with_selected(s.place == place),
        );
    }
    root
}

/// This PC: the drives and their folders as far as they were listed.
fn this_pc_tree(s: &DriveState, places: &mut Vec<Place>) -> TreeViewNode {
    places.push(Place::ThisPc);
    let mut root = TreeViewNode::create(AzString::from(browse::THIS_PC))
        .with_icon(AzString::from("computer"))
        .with_expanded(s.tree.this_pc_open)
        .with_selected(s.place == Place::ThisPc);
    for slot in &s.slots {
        root = root.with_child(folder_node(
            s,
            &slot.entry.id,
            "",
            &slot.entry.name,
            slot.icon(),
            places,
        ));
    }
    root
}

fn folder_node(
    s: &DriveState,
    drive: &str,
    prefix: &str,
    label: &str,
    icon: &str,
    places: &mut Vec<Place>,
) -> TreeViewNode {
    let place = Place::folder(drive, prefix);
    let node_key = (drive.to_string(), prefix.to_string());
    places.push(place.clone());
    let loaded = s.tree.loaded.get(&node_key);
    let mut node = TreeViewNode::create(AzString::from(label))
        .with_icon(AzString::from(icon))
        .with_expanded(s.tree.expanded.contains(&node_key))
        .with_selected(s.place == place)
        .with_unloaded_children(loaded.is_none());
    if let Some(folders) = loaded {
        for folder in folders {
            node = node.with_child(folder_node(
                s,
                drive,
                folder,
                key::last_segment(folder),
                "folder",
                places,
            ));
        }
    }
    node
}

/// Explorer's navigation pane: Quick access and This PC, each a group with
/// its tree (the drives' folders listed lazily, one listing per expand).
pub(crate) fn navigation_pane(s: &DriveState, app: &RefAny) -> Dom {
    let mut places = Vec::new();
    let this_pc = this_pc_tree(s, &mut places);
    ShellNavigationPane::create()
        .with_label(AzString::from("Navigation pane"))
        .with_group(
            ShellNavigationGroup::create(AzString::from(browse::QUICK_ACCESS), quick_access_tree(s))
                .with_count(s.settings.pinned.len())
                .with_open(!s.tree.groups_closed[0]),
        )
        .with_group(
            ShellNavigationGroup::create(AzString::from(browse::THIS_PC), this_pc)
                .with_count(s.slots.len())
                .with_open(!s.tree.groups_closed[1]),
        )
        .with_on_event(app.clone(), on_nav_event as ShellNavigationPaneOnEventCallbackType)
        .dom()
}

extern "C" fn on_nav_event(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: ShellNavigationPaneEvent,
) -> Update {
    let mods = info.get_key_modifiers();
    let copy = mods.ctrl || mods.meta;
    with_state(&mut data, &mut info, |info, app, s| {
        let group = event.group.min(1);
        match event.kind {
            ShellNavigationPaneEventKind::GroupToggled => {
                s.tree.groups_closed[group] = !event.expand;
            }
            ShellNavigationPaneEventKind::NodeClicked => {
                if let Some(place) = group_places(s, group).get(event.index).cloned() {
                    go(info, app, s, place, true);
                }
            }
            ShellNavigationPaneEventKind::NodeToggled => {
                match group_places(s, group).get(event.index).cloned() {
                    Some(Place::QuickAccess) => s.tree.quick_open = event.expand,
                    Some(Place::ThisPc) => s.tree.this_pc_open = event.expand,
                    Some(Place::Folder { drive, prefix }) => {
                        let node = (drive, prefix);
                        if event.expand {
                            s.tree.expanded.insert(node.clone());
                            if !s.tree.loaded.contains_key(&node) {
                                start_tree_listing(info, app, s, node);
                            }
                        } else {
                            s.tree.expanded.remove(&node);
                        }
                    }
                    None => {}
                }
            }
            // Items dragged in the window, dropped on a folder of the tree.
            ShellNavigationPaneEventKind::NodeDropped => {
                if let Some(place) = group_places(s, group).get(event.index).cloned() {
                    actions::drop_on_place(info, app, s, place, copy);
                }
            }
            ShellNavigationPaneEventKind::ModuleSelected
            | ShellNavigationPaneEventKind::CollapseToggled => {}
        }
    })
}

// ==== The status bar ====

/// "12 items", "3 items selected 1.5 MB", the running transfer, Explorer's
/// Details / Large icons switch.
pub(crate) fn status_bar(s: &DriveState, app: &RefAny) -> Dom {
    let mut segments = Vec::new();
    let count = match &s.place {
        Place::ThisPc => format!("{} drives", s.slots.len()),
        Place::QuickAccess => format!("{} pinned folders", s.settings.pinned.len()),
        Place::Folder { .. } if s.loading => String::from("Loading..."),
        Place::Folder { .. } => {
            let shown = s.visible_entries().len();
            let more = if s.next.is_some() { "+" } else { "" };
            format!("{shown}{more} items")
        }
    };
    segments.push(StatusBarSegment::create(AzString::from(count)).with_marker(AzString::from("items")));
    let selected = s.selected_entries();
    if !selected.is_empty() {
        let bytes: u64 = selected.iter().filter_map(|e| e.size).sum();
        let mut text = format!("{} items selected", selected.len());
        if bytes > 0 {
            text.push_str(&format!("  {}", browse::format_size(Some(bytes))));
        }
        segments.push(
            StatusBarSegment::create(AzString::from(text)).with_marker(AzString::from("selected")),
        );
    }
    let transfer = s.queue.status_text();
    if !transfer.is_empty() {
        segments.push(
            StatusBarSegment::create(AzString::from(transfer))
                .with_icon(AzString::from("sync"))
                .with_marker(AzString::from("transfer"))
                .with_on_click(action_ref(app, Action::ShowTransfers), on_action),
        );
    }
    let failed = s.queue.failed().len();
    if failed > 0 {
        segments.push(
            StatusBarSegment::create(AzString::from(format!("{failed} transfer(s) failed")))
                .with_icon(AzString::from("error"))
                .with_on_click(action_ref(app, Action::ShowTransfers), on_action),
        );
    }
    if s.clipboard.is_some() {
        let clip = s.clipboard.as_ref().map_or(0, |c| c.items.len());
        segments.push(StatusBarSegment::create(AzString::from(format!(
            "{clip} on the clipboard"
        ))));
    }
    let active = match s.settings.layout {
        ViewLayout::Details => 0,
        ViewLayout::LargeIcons => 1,
        _ => 2,
    };
    let views = StatusBarViewSwitcher::create(StatusBarViewVec::from(vec![
        StatusBarView {
            icon: AzString::from("view_headline"),
        },
        StatusBarView {
            icon: AzString::from("grid_view"),
        },
    ]))
    .with_active_view(active)
    .with_on_select(app.clone(), on_view_select as StatusBarOnViewSelectCallbackType);
    StatusBar::create(StatusBarSegmentVec::from(segments))
        .with_views(views)
        .dom()
}

extern "C" fn on_view_select(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        let layout = if index == 0 {
            ViewLayout::Details
        } else {
            ViewLayout::LargeIcons
        };
        actions::set_layout(info, app, s, layout);
    })
}

// ==== The preview pane ====

/// One line of the preview pane.
fn note(text: &str) -> Dom {
    Dom::create_div()
        .with_css("padding: 16px; font-size: 13px; opacity: 0.75;")
        .with_child(Dom::create_span_with_text(AzString::from(text)))
}

/// Play / Stop of a WAV preview: its samples into azul's AudioSink. Play
/// after the sound ended starts it again.
extern "C" fn on_play_audio(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| {
        let Some(PreviewContent::Audio(wav)) =
            s.preview.as_ref().and_then(|p| p.content.clone())
        else {
            return;
        };
        if let Some(mut sink) = s.audio.take() {
            let ended = sink.frames_played() >= wav.frames();
            sink.close();
            if !ended {
                return;
            }
        }
        let sink = AudioSink::open(AudioConfig {
            sample_rate: wav.sample_rate,
            channels: wav.channels,
        });
        if !sink.is_open() {
            let why = sink
                .error_message()
                .into_option()
                .map(|e| e.as_str().to_string())
                .unwrap_or_else(|| String::from("no audio output"));
            s.error(format!("The sound cannot play: {why}"));
            return;
        }
        sink.play(AudioFrame {
            sample_rate: wav.sample_rate,
            channels: wav.channels,
            samples: F32Vec::from(wav.samples),
        });
        println!("AZDRIVE_DONE playing {}", wav.sample_rate);
        s.audio = Some(sink);
    })
}

/// The preview pane: the selected file's picture, text, PDF page, sound or
/// video.
pub(crate) fn preview_pane(s: &DriveState, app: &RefAny, _dark: bool) -> Dom {
    let body = match &s.preview {
        None => note("Select a file to preview."),
        Some(preview) => {
            let name = key::last_segment(&preview.key).to_string();
            match &preview.content {
                None => note(&format!("Loading the preview of \"{name}\"...")),
                Some(PreviewContent::Message(text)) => note(text),
                Some(PreviewContent::Text(text)) => Dom::create_div()
                    .with_id("preview-text")
                    .with_css(
                        "padding: 8px 12px; font-family: monospace; font-size: 12px; \
                         white-space: pre-wrap; overflow-y: auto; flex-grow: 1; min-height: 0px;",
                    )
                    .with_child(Dom::create_span_with_text(AzString::from(text.as_str()))),
                Some(PreviewContent::Image {
                    image,
                    width,
                    height,
                }) => Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: column; align-items: center; padding: 12px; \
                         overflow-y: auto; flex-grow: 1; min-height: 0px;",
                    )
                    .with_child(
                        Dom::create_image(image.clone())
                            .with_id("preview-image")
                            .with_css("max-width: 100%; max-height: 420px;"),
                    )
                    .with_child(
                        Dom::create_span_with_text(AzString::from(format!(
                            "{name} - {width} x {height} pixels"
                        )))
                        .with_css("margin-top: 8px; font-size: 12px; opacity: 0.75;"),
                    ),
                Some(PreviewContent::Audio(wav)) => {
                    let seconds = wav.seconds();
                    let minutes = (seconds / 60.0).floor() as u64;
                    let rest = seconds - minutes as f64 * 60.0;
                    let channels = match wav.channels {
                        1 => String::from("mono"),
                        2 => String::from("stereo"),
                        n => format!("{n} channels"),
                    };
                    let playing = s
                        .audio
                        .as_ref()
                        .is_some_and(|sink| sink.frames_played() < wav.frames());
                    Dom::create_div()
                        .with_id("preview-audio")
                        .with_css("display: flex; flex-direction: column; padding: 16px;")
                        .with_child(
                            Dom::create_icon(AzString::from("music_note"))
                                .with_css("font-size: 64px; opacity: 0.6;"),
                        )
                        .with_child(Dom::create_span_with_text(AzString::from(format!(
                            "{name} - {minutes}:{rest:04.1} ({:.1} kHz, {channels})",
                            f64::from(wav.sample_rate) / 1000.0
                        ))))
                        .with_child(
                            Button::create(AzString::from(if playing { "Stop" } else { "Play" }))
                                .with_icon(AzString::from(if playing {
                                    "stop"
                                } else {
                                    "play_arrow"
                                }))
                                .with_on_click(
                                    app.clone(),
                                    on_play_audio as ButtonOnClickCallbackType,
                                )
                                .dom()
                                .with_id("preview-play")
                                .with_css("margin-top: 12px; align-self: flex-start;"),
                        )
                }
                Some(PreviewContent::Video(path)) => {
                    let config = VideoConfig {
                        source: VideoSource::File(AzString::from(path.display().to_string())),
                        timestamp: 0.0,
                        autoplay: true,
                        looping: true,
                        paused: false,
                        output_format: RawImageFormat::BGRA8,
                    };
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: column; padding: 12px;")
                        .with_child(
                            VideoWidget::create(config)
                                .dom()
                                .with_id("preview-video")
                                .with_css("width: 100%; height: 240px;"),
                        )
                        .with_child(
                            Dom::create_span_with_text(AzString::from(format!(
                                "{name} (H.264, without sound: azul's video widget has no audio \
                                 track)"
                            )))
                            .with_css("margin-top: 8px; font-size: 12px; opacity: 0.75;"),
                        )
                }
            }
        }
    };
    Dom::create_div()
        .with_id("preview-pane")
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(body)
}

// ==== The details pane ====

/// The details pane: the selected items, the open folder, or a drive.
pub(crate) fn details_pane(s: &DriveState) -> Dom {
    let pane = match &s.place {
        Place::ThisPc => match s.selected_drive.and_then(|i| s.slots.get(i)) {
            Some(slot) => {
                let mut pane = DetailsPane::create(AzString::from(slot.entry.name.as_str()))
                    .with_icon(AzString::from(slot.icon()))
                    .with_subtitle(AzString::from(slot.kind()));
                match &slot.entry.location {
                    DriveLocation::Local { root } => {
                        if let Some((total, free)) = s.disk.get(&slot.entry.id) {
                            pane = pane
                                .with_property(
                                    AzString::from("Space used"),
                                    AzString::from(browse::format_size(Some(
                                        total.saturating_sub(*free),
                                    ))),
                                )
                                .with_property(
                                    AzString::from("Free space"),
                                    AzString::from(browse::format_size(Some(*free))),
                                )
                                .with_property(
                                    AzString::from("Total size"),
                                    AzString::from(browse::format_size(Some(*total))),
                                );
                        }
                        pane.with_property(AzString::from("Path"), AzString::from(root.as_str()))
                    }
                    DriveLocation::S3 {
                        endpoint,
                        region,
                        bucket,
                        ..
                    } => pane
                        .with_property(AzString::from("Bucket"), AzString::from(bucket.as_str()))
                        .with_property(
                            AzString::from("Endpoint"),
                            AzString::from(endpoint.as_str()),
                        )
                        .with_property(AzString::from("Region"), AzString::from(region.as_str())),
                }
            }
            None => DetailsPane::create(AzString::from(browse::THIS_PC))
                .with_icon(AzString::from("computer"))
                .with_subtitle(AzString::from(format!("{} drives", s.slots.len()))),
        },
        Place::QuickAccess => DetailsPane::create(AzString::from(browse::QUICK_ACCESS))
            .with_icon(AzString::from("star"))
            .with_subtitle(AzString::from(format!(
                "{} pinned folders",
                s.settings.pinned.len()
            ))),
        Place::Folder { drive, .. } => {
            let selected = s.selected_entries();
            match selected.as_slice() {
                [] => DetailsPane::create(AzString::from(s.place_name()))
                    .with_icon(AzString::from("folder_open"))
                    .with_subtitle(AzString::from("File folder"))
                    .with_property(
                        AzString::from("Items"),
                        AzString::from(s.visible_entries().len().to_string()),
                    )
                    .with_property(
                        AzString::from("Location"),
                        AzString::from(actions::item_location(s, drive, s.prefix())),
                    ),
                [entry] => {
                    let mut pane = DetailsPane::create(AzString::from(entry.name.as_str()))
                        .with_icon(AzString::from(ui_view::icon_for(entry)))
                        .with_subtitle(AzString::from(entry.kind()));
                    if !entry.is_folder {
                        pane = pane
                            .with_property(
                                AzString::from("Size"),
                                AzString::from(browse::format_size(entry.size)),
                            )
                            .with_property(
                                AzString::from("Date modified"),
                                AzString::from(browse::format_modified(
                                    entry.modified,
                                    &chrono::Local,
                                )),
                            );
                    }
                    pane = pane.with_property(
                        AzString::from("Location"),
                        AzString::from(actions::item_location(s, drive, &entry.key)),
                    );
                    if let Some(etag) = &entry.etag {
                        pane = pane.with_property(AzString::from("ETag"), AzString::from(etag.as_str()));
                    }
                    if let Some(Ok(pairs)) = s.metadata.get(&entry.key) {
                        for (name, value) in pairs {
                            if name != "ETag" {
                                pane = pane.with_property(
                                    AzString::from(name.as_str()),
                                    AzString::from(value.as_str()),
                                );
                            }
                        }
                    }
                    pane
                }
                many => {
                    let bytes: u64 = many.iter().filter_map(|e| e.size).sum();
                    let folders = many.iter().filter(|e| e.is_folder).count();
                    DetailsPane::create(AzString::from(format!("{} items selected", many.len())))
                        .with_icon(AzString::from("library_add_check"))
                        .with_property(
                            AzString::from("Files"),
                            AzString::from((many.len() - folders).to_string()),
                        )
                        .with_property(AzString::from("Folders"), AzString::from(folders.to_string()))
                        .with_property(
                            AzString::from("Size of the files"),
                            AzString::from(browse::format_size(Some(bytes))),
                        )
                }
            }
        }
    };
    pane.dom().with_id("details")
}
