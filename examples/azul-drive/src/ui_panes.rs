//! The panes around the content: the navigation row (the address bar), the preview pane and the
//! details pane. (The navigation pane is `ui_sidebar`'s.) The leaf has no path bar and no
//! status line at its foot: the address row says where the window is, and the sync and the
//! drive's space are the navigation pane's pies.

use azul::{
    audio::{AudioConfig, AudioFrame, AudioSink},
    callbacks::{AddressBarOnEventCallbackType, ButtonOnClickCallbackType},
    image::RawImageFormat,
    menu::MenuItem,
    prelude::*,
    str::String as AzString,
    vec::{F32Vec, StringVec},
    video::{VideoConfig, VideoSource},
    widgets::{AddressBar, AddressBarEvent, AddressBarEventKind, DetailsPane, VideoWidget},
};
use azul_appkit::l10n::{grouped, label, t, t_args, t_text, Arg, Phrase, Text};
use azul_storage::{config::DriveLocation, key};

use crate::{
    actions::{self, menu_item, Action},
    browse::{self, Place},
    go, ids,
    jobs::PreviewContent,
    look, place_up, start_tree_listing, ui_view, with_state, DriveState,
};

// ==== The address bar ====

/// Explorer's address row (azul's AddressBar): round Back and Forward, Up (the design has no
/// Recent locations button: Back and Forward keep the way), the
/// breadcrumb box - the place's icon, a crumb per step with a chevron that drops its folders,
/// the first steps folded into « when the path is long, a click on the empty part for the typed
/// path, Refresh at its end - and "Search <folder>". `width` is the window's (the bar folds its
/// crumbs to it).
pub(crate) fn address_bar(s: &DriveState, app: &RefAny, width: f32) -> Dom {
    bar_model(s, width)
        .with_on_event(app.clone(), on_address as AddressBarOnEventCallbackType)
        .dom()
}

/// How many crumbs of the open place fold into the bar's « menu in a window `width` px wide:
/// the one thing of the window's build that follows its width (the panes keep their widths and
/// the folder view lays its rows out for its box by itself).
pub(crate) fn folded_crumbs(s: &DriveState, width: f32) -> usize {
    bar_model(s, width).folded_crumbs_at(width)
}

/// The address bar of the open place, for a window `width` px wide, without its callback.
fn bar_model(s: &DriveState, width: f32) -> AddressBar {
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
        .with_icon(AzString::from(crumb_icon(s, &s.place)))
        // The git chip at the row's end takes its share of the width (`ui_git`).
        .with_available_width((width - crate::ui_git::chip_room(s)).max(1.0))
        .with_path(AzString::from(browse::path_text(&s.place, path_drive)))
        .with_search(AzString::from(s.search.as_str()))
        .with_search_placeholder(AzString::from(t_args(
            "azdrive-search-placeholder",
            &[("place", Arg::from(s.place_name()))],
        )))
        .with_can_go(
            s.history.can_go_back(),
            s.history.can_go_forward(),
            place_up(&s.place).is_some(),
        )
        .with_editing(s.editing_path)
        .with_recent(false)
        // The design's key cap in the search box (it hides while the box has the focus).
        .with_search_shortcut(AzString::from(if cfg!(target_os = "macos") {
            "\u{2318}F"
        } else {
            "Ctrl+F"
        }))
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
                .map(|slot| (slot.name(), Place::folder(&slot.entry.id, "")))
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
                if place == s.place {
                    // The open folder's own crumb: Explorer reads the folder again.
                    crate::refresh(info, app, s);
                } else {
                    go(info, app, s, place, true);
                }
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
                    actions::open_menu_below(info, items);
                }
                Some(_) => s.info(Text::key("azdrive-no-subfolders")),
                None => {
                    if let Place::Folder { drive, prefix } = place {
                        s.info(Text::key("azdrive-listing-folder"));
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
                None => s.error(Phrase::new("azdrive-no-drive-for").arg("path", text.trim())),
            }
        }
        AddressBarEventKind::EditCancelled => s.editing_path = false,
        AddressBarEventKind::Search => {
            s.search = text.clone();
            // The search running stops; one for the new text starts (none for an empty box):
            // the open folder and every folder below it.
            crate::start_find(info, app, s);
            let keys = s.visible_keys();
            let order: Vec<&str> = keys.iter().map(String::as_str).collect();
            s.selection.retain(&order);
            // Other rows are in view now: their sizes, counts and thumbnails.
            actions::request_view_work(info, app, s);
        }
    })
}

/// The icon of a place in the address bar: This PC's, a drive's own, a folder.
fn crumb_icon(s: &DriveState, place: &Place) -> &'static str {
    match place {
        Place::QuickAccess => "star",
        Place::ThisPc => "computer",
        Place::Folder { drive, prefix } if prefix.is_empty() => s
            .slot_index(drive)
            .map_or("storage", |i| s.slots[i].icon()),
        Place::Folder { .. } => "folder",
    }
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
                .unwrap_or_else(|| t("azdrive-audio-no-output"));
            s.error(Phrase::new("azdrive-audio-cannot-play").arg("why", why));
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
        None => note(&t("azdrive-preview-select")),
        Some(preview) => {
            let name = key::last_segment(&preview.key).to_string();
            match &preview.content {
                None => note(&t_args("azdrive-preview-loading", &[("name", Arg::from(name))])),
                Some(PreviewContent::Message(text)) => note(&t_text(text)),
                Some(PreviewContent::Text { text, cut }) => {
                    // A cut file says it is its start.
                    let shown = if *cut {
                        format!("{text}\n{}", t("azdrive-preview-cut"))
                    } else {
                        text.clone()
                    };
                    Dom::create_div()
                        .with_id(ids::PREVIEW_TEXT)
                        .with_css(
                            "padding: 8px 12px; font-family: monospace; font-size: 12px; \
                             white-space: pre-wrap; overflow-y: auto; flex-grow: 1; \
                             min-height: 0px;",
                        )
                        .with_child(Dom::create_span_with_text(AzString::from(shown)))
                }
                Some(PreviewContent::Image {
                    image,
                    width,
                    height,
                }) => {
                    // The picture (a PDF's first page, an image) as a card on the stage; its size
                    // is in the facts under it, not in a caption.
                    let _ = (width, height);
                    Dom::create_image(image.clone())
                        .with_id(ids::PREVIEW_IMAGE)
                        .with_css(
                            "max-width: 100%; max-height: 100%; border-radius: 2px; \
                             box-shadow: 0px 1px 2px rgba(0,0,0,0.2), 0px 6px 18px rgba(0,0,0,0.18);",
                        )
                }
                Some(PreviewContent::Audio(wav)) => {
                    let seconds = wav.seconds();
                    let minutes = (seconds / 60.0).floor() as u64;
                    let rest = seconds - minutes as f64 * 60.0;
                    let channels = match wav.channels {
                        1 => t("azdrive-preview-mono"),
                        2 => t("azdrive-preview-stereo"),
                        n => t_args(
                            "azdrive-preview-channels",
                            &[("count", Arg::from(u32::from(n)))],
                        ),
                    };
                    let playing = s
                        .audio
                        .as_ref()
                        .is_some_and(|sink| sink.frames_played() < wav.frames());
                    Dom::create_div()
                        .with_id(ids::PREVIEW_AUDIO)
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
                            Button::create(label(if playing {
                                "azdrive-preview-stop"
                            } else {
                                "azdrive-preview-play"
                            }))
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
                                .with_id(ids::PREVIEW_PLAY)
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
                                .with_id(ids::PREVIEW_VIDEO)
                                .with_css("width: 100%; height: 240px;"),
                        )
                        .with_child(
                            Dom::create_span_with_text(AzString::from(t_args(
                                "azdrive-preview-video-note",
                                &[("name", Arg::from(name.as_str()))],
                            )))
                            .with_css("margin-top: 8px; font-size: 12px; opacity: 0.75;"),
                        )
                }
            }
        }
    };
    let mut pane = Dom::create_div()
        .with_id(ids::PREVIEW_PANE)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(
            Dom::create_div()
                .with_css(look::PREVIEW_STAGE)
                .with_child(body),
        );
    if let Some(info) = preview_info(s, app) {
        pane.add_child(info);
    }
    pane
}

/// Under the preview: the item's name and kind, what it is (Type, Size, Modified, Location,
/// and where something syncs its state), and at the foot Open and Copy link.
fn preview_info(s: &DriveState, app: &RefAny) -> Option<Dom> {
    let entry = s.single_selected()?;
    let kind = crate::model::cell_text(entry, browse::Column::Type, s.settings.show_extensions);
    let row = |key: &str, value: String| {
        Dom::create_div()
            .with_css(look::PREVIEW_ROW)
            .with_child(
                Dom::create_div()
                    .with_css(look::PREVIEW_ROW_KEY)
                    .with_child(Dom::create_span_with_text(label(key))),
            )
            .with_child(
                Dom::create_div()
                    .with_css(look::PREVIEW_ROW_VALUE)
                    .with_child(Dom::create_span_with_text(AzString::from(value))),
            )
    };
    let drive_name = s.drive_name(&s.place);
    let location = browse::path_text(
        &Place::folder(
            &s.current_drive_id().unwrap_or_default(),
            &crate::fileops::parent_of(&entry.key),
        ),
        Some(&drive_name),
    );
    let mut list = Dom::create_div()
        .with_id(ids::PREVIEW_INFO)
        .with_css(look::PREVIEW_INFO)
        .with_child(
            Dom::create_div()
                .with_css(look::PREVIEW_NAME)
                .with_child(Dom::create_span_with_text(AzString::from(
                    entry.display_name(s.settings.show_extensions),
                ))),
        )
        .with_child(
            Dom::create_div()
                .with_css(look::PREVIEW_KIND)
                .with_child(Dom::create_span_with_text(AzString::from(kind.as_str()))),
        )
        .with_child(row("azdrive-column-type", kind.clone()))
        .with_child(row("azdrive-column-size", ui_view::size_text(s, entry)))
        .with_child(row(
            "azdrive-column-modified",
            browse::format_modified(entry.modified, &chrono::Local),
        ))
        .with_child(row("azdrive-meta-location", location));
    if let Some((_, words, _)) = crate::sync_view::badge(s, entry) {
        list.add_child(row("azdrive-find-column-status", words));
    }
    let button = |text: &str, action: Action| {
        Button::create(label(text))
            .with_on_click(
                actions::action_ref(app, action),
                actions::on_action as ButtonOnClickCallbackType,
            )
            .dom()
    };
    list.add_child(
        Dom::create_div()
            .with_css(look::PREVIEW_ACTIONS)
            .with_child(button("azdrive-ribbon-open", Action::Open))
            .with_child(button("azdrive-ribbon-copy-link", Action::Share)),
    );
    Some(list)
}

// ==== The details pane ====

/// The details pane: the selected items, the open folder, or a drive.
pub(crate) fn details_pane(s: &DriveState) -> Dom {
    let pane = match &s.place {
        Place::ThisPc => match s.selected_drive.and_then(|i| s.slots.get(i)) {
            Some(slot) => {
                let mut pane = DetailsPane::create(AzString::from(slot.name()))
                    .with_icon(AzString::from(slot.icon()))
                    .with_subtitle(AzString::from(slot.kind()));
                // An encrypted drive's recovery: green, yellow or red, and why (D51).
                if let Some(health) = crate::recovery_health::health_line(
                    &s.settings.recovery.drives,
                    &slot.entry.id,
                    azul_storage::time::now_unix(),
                ) {
                    pane = pane.with_property(
                        label("azdrive-details-recovery-health"),
                        AzString::from(t_text(&health)),
                    );
                }
                // An Azlin drive's space: its stored bytes against the quota, the files' size
                // before compression as extra information.
                if let Some(usage) = crate::usage_view::usage_of_slot(s, &slot.entry.id)
                    .map(crate::usage_view::DriveUsage::usage)
                    .filter(|usage| usage.quota > 0)
                {
                    pane = pane.with_property(
                        label("azdrive-details-used"),
                        AzString::from(crate::usage_view::usage_text(&usage)),
                    );
                }
                match &slot.entry.location {
                    DriveLocation::Local { root } => {
                        if let Some((total, free)) = s.disk.get(&slot.entry.id) {
                            pane = pane
                                .with_property(
                                    label("azdrive-details-used"),
                                    AzString::from(browse::format_size(Some(
                                        total.saturating_sub(*free),
                                    ))),
                                )
                                .with_property(
                                    label("azdrive-details-free"),
                                    AzString::from(browse::format_size(Some(*free))),
                                )
                                .with_property(
                                    label("azdrive-details-total"),
                                    AzString::from(browse::format_size(Some(*total))),
                                );
                        }
                        pane.with_property(
                            label("azdrive-details-path"),
                            AzString::from(root.as_str()),
                        )
                    }
                    DriveLocation::S3 {
                        endpoint,
                        region,
                        bucket,
                        ..
                    } => pane
                        .with_property(
                            label("azdrive-props-bucket"),
                            AzString::from(bucket.as_str()),
                        )
                        .with_property(
                            label("azdrive-props-endpoint"),
                            AzString::from(endpoint.as_str()),
                        )
                        .with_property(
                            label("azdrive-props-region"),
                            AzString::from(region.as_str()),
                        ),
                    DriveLocation::Opendal { options, .. }
                    | DriveLocation::Database { options, .. } => {
                        let mut pane = pane;
                        for (name, value) in crate::ui_dialogs::source_rows(&slot.entry, options) {
                            pane = pane.with_property(label(&name), AzString::from(value));
                        }
                        pane
                    }
                }
            }
            None => DetailsPane::create(label("azdrive-this-pc"))
                .with_icon(AzString::from("computer"))
                .with_subtitle(AzString::from(t_args(
                    "azdrive-status-drives",
                    &[("count", Arg::from(s.slots.len()))],
                ))),
        },
        Place::QuickAccess => DetailsPane::create(label("azdrive-quick-access"))
            .with_icon(AzString::from("star"))
            .with_subtitle(AzString::from(t_args(
                "azdrive-status-pins",
                &[("count", Arg::from(s.settings.pinned.len()))],
            ))),
        Place::Folder { drive, .. } => {
            let selected = s.selected_entries();
            match selected.as_slice() {
                [] => DetailsPane::create(AzString::from(s.place_name()))
                    .with_icon(AzString::from("folder_open"))
                    .with_subtitle(label("azdrive-details-file-folder"))
                    .with_property(
                        label("azdrive-details-items"),
                        AzString::from(grouped(s.visible_entries().len() as u64)),
                    )
                    .with_property(
                        label("azdrive-props-location"),
                        AzString::from(actions::item_location(s, drive, s.prefix())),
                    ),
                [entry] => {
                    let mut pane = DetailsPane::create(AzString::from(entry.name.as_str()))
                        .with_icon(AzString::from(ui_view::icon_for(entry)))
                        .with_subtitle(AzString::from(entry.kind()));
                    if !entry.is_folder {
                        pane = pane
                            .with_property(
                                label("azdrive-props-size"),
                                AzString::from(browse::format_size(entry.size)),
                            )
                            .with_property(
                                label("azdrive-find-column-modified"),
                                AzString::from(browse::format_modified(
                                    entry.modified,
                                    &chrono::Local,
                                )),
                            );
                    } else if let Some(n) = s.counts.get(&entry.key) {
                        // Counted with one read of the folder, no stat per item.
                        pane = pane.with_property(
                            label("azdrive-details-items"),
                            AzString::from(grouped(*n as u64)),
                        );
                    }
                    pane = pane.with_property(
                        label("azdrive-props-location"),
                        AzString::from(actions::item_location(s, drive, &entry.key)),
                    );
                    if let Some(etag) = &entry.etag {
                        pane = pane.with_property(AzString::from("ETag"), AzString::from(etag.as_str()));
                    }
                    if let Some(Ok(pairs)) = s.metadata.get(&entry.key) {
                        let shown = ["Size", "Date modified", "Location", "ETag"];
                        for (name, value) in browse::metadata_rows(pairs, &shown, &chrono::Local) {
                            let (name, value) = crate::source_words::meta_row(&name, &value);
                            pane = pane.with_property(AzString::from(name), AzString::from(value));
                        }
                    }
                    pane
                }
                many => {
                    let bytes: u64 = many.iter().filter_map(|e| e.size).sum();
                    let folders = many.iter().filter(|e| e.is_folder).count();
                    DetailsPane::create(AzString::from(t_args(
                        "azdrive-details-selected",
                        &[("count", Arg::from(many.len()))],
                    )))
                    .with_icon(AzString::from("library_add_check"))
                    .with_property(
                        label("azdrive-details-files"),
                        AzString::from((many.len() - folders).to_string()),
                    )
                    .with_property(
                        label("azdrive-details-folders"),
                        AzString::from(folders.to_string()),
                    )
                    .with_property(
                        label("azdrive-props-size-of-files"),
                        AzString::from(browse::format_size(Some(bytes))),
                    )
                }
            }
        }
    };
    pane.dom().with_id(ids::DETAILS).with_css(look::DETAILS_FILL)
}
