//! The panes around the content: the navigation row (the address bar), the
//! path bar and the status line at the foot of the content's leaf, the preview
//! pane and the details pane. (The navigation pane is `ui_sidebar`'s.)

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
use azul_appkit::l10n::{grouped, label, t, t_args, t_phrase, t_text, Arg, Phrase, Text};
use azul_storage::{config::DriveLocation, key};

use crate::{
    actions::{self, action_ref, menu_item, on_action, Action},
    browse::{self, Place},
    go, ids,
    jobs::PreviewContent,
    listing, look, place_up, start_tree_listing, ui_view, with_state, DriveState,
};

// ==== The address bar ====

/// Explorer's address row (azul's AddressBar): round Back and Forward, Recent locations, Up, the
/// breadcrumb box - the place's icon, a crumb per step with a chevron that drops its folders,
/// the first steps folded into « when the path is long, a click on the empty part for the typed
/// path, Refresh at its end - and "Search <folder>". `width` is the window's (the bar folds its
/// crumbs to it).
pub(crate) fn address_bar(s: &DriveState, app: &RefAny, width: f32) -> Dom {
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
        .with_available_width(width)
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

// ==== The path bar and the status line (the leaf's foot) ====

/// What a step of the path bar carries: the app and the step's place.
struct CrumbRef {
    app: RefAny,
    place: Place,
}

fn crumb_parts(data: &mut RefAny) -> Option<(RefAny, Place)> {
    data.downcast_ref::<CrumbRef>()
        .map(|c| (c.app.clone(), c.place.clone()))
}

/// The icon of a step of the path bar: This PC's, a drive's own, a folder.
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

/// Finder's path bar at the foot of the leaf: the open place's trail (This PC, the drive, its
/// folders), each step its icon and its name, the open place's own in bold. A click goes there;
/// items dragged in the window and dropped on a step move there (Ctrl copies).
pub(crate) fn path_bar(s: &DriveState, app: &RefAny) -> Dom {
    let drive_name = s.drive_name(&s.place);
    let crumbs = browse::crumbs_of(&s.place, &drive_name);
    let last = crumbs.len().saturating_sub(1);
    let mut bar = Dom::create_div()
        .with_id(ids::PATH_BAR)
        .with_css(look::PATH_BAR)
        .with_accessibility_name(label("azdrive-path-bar"));
    for (i, (label, place)) in crumbs.into_iter().enumerate() {
        if i > 0 {
            bar.add_child(Dom::create_icon("chevron_right").with_css(look::CRUMB_SEPARATOR));
        }
        let data = || {
            RefAny::new(CrumbRef {
                app: app.clone(),
                place: place.clone(),
            })
        };
        let mut crumb = Dom::create_div()
            .with_class(ids::CRUMB_CLASS)
            .with_css(format!(
                "{} {}",
                look::CRUMB,
                if i == last { look::CRUMB_LAST } else { "" }
            ))
            .with_accessibility_name(label.as_str())
            .with_child(Dom::create_icon(crumb_icon(s, &place)).with_css(look::CRUMB_ICON))
            .with_child(
                Dom::create_div()
                    .with_css(
                        "min-width: 0px; overflow: hidden; white-space: nowrap; \
                         text-overflow: ellipsis;",
                    )
                    .with_child(Dom::create_span_with_text(AzString::from(label.as_str()))),
            )
            .with_callback(EventFilter::Hover(HoverEventFilter::Click), data(), on_crumb);
        if matches!(place, Place::Folder { .. }) {
            crumb = crumb
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::DragOver),
                    data(),
                    on_crumb_drag_over,
                )
                .with_callback(EventFilter::Hover(HoverEventFilter::Drop), data(), on_crumb_drop);
        }
        bar.add_child(crumb);
    }
    bar
}

/// A step of the path bar was clicked: the window goes there.
extern "C" fn on_crumb(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, place)) = crumb_parts(&mut data) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        if place != s.place {
            go(info, app, s, place, true);
        }
    })
}

extern "C" fn on_crumb_drag_over(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.accept_drop();
    Update::DoNothing
}

/// Items dragged in the window, dropped on a step: into its folder (Ctrl copies).
extern "C" fn on_crumb_drop(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, place)) = crumb_parts(&mut data) else {
        return Update::DoNothing;
    };
    info.stop_propagation();
    let copy = info.get_key_modifiers().primary_down();
    with_state(&mut app, &mut info, |info, app, s| {
        actions::drop_on_place(info, app, s, place, copy);
    })
}

/// The status line's text: how many items (drives, pinned folders) and how many of them are
/// selected (their size), what the clipboard holds, the space the drive has left - Finder's
/// "3 of 12 selected (1.5 MB), 45.3 GB available".
pub(crate) fn status_text(s: &DriveState) -> String {
    let mut parts = Vec::new();
    // The drive in view's storage problem first, in the table's words with its error ID.
    if let Some(problem) = crate::problems::status_of(s) {
        parts.push(t_text(&problem));
    }
    if let Some(find) = &s.find {
        // The search's own line: "Searching... 1,234 found", then the count; the refine.
        parts.push(t_phrase(&find.status_text()));
        if !s.refines.is_any() {
            parts.push(t_text(&s.refines.label()));
        }
        // The drive's index, when it has one: how far its update got, what it holds.
        if let Some(drive_id) = s
            .current_drive_id()
            .filter(|id| s.settings.indexed_drives.contains(id))
        {
            parts.push(s.indexes.get(&drive_id).map_or_else(
                || t("azdrive-index-status-never"),
                |info| t_phrase(&info.status_text()),
            ));
        }
        let selected = s.selection.len();
        if selected > 0 {
            parts.push(t_args(
                "azdrive-status-selected",
                &[
                    ("count", Arg::from(selected)),
                    ("n", Arg::from(grouped(selected as u64))),
                ],
            ));
        }
        if let Some(clip) = &s.clipboard {
            parts.push(clipboard_text(clip.items.len()));
        }
        return parts.join(", ");
    }
    match &s.place {
        Place::ThisPc => {
            let drives = t_args("azdrive-status-drives", &[("count", Arg::from(s.slots.len()))]);
            parts.push(match s.selected_drive.and_then(|i| s.slots.get(i)) {
                Some(slot) => t_args(
                    "azdrive-status-drive-selected",
                    &[
                        ("name", Arg::from(slot.entry.name.as_str())),
                        ("drives", Arg::from(drives)),
                    ],
                ),
                None => drives,
            });
        }
        Place::QuickAccess => parts.push(t_args(
            "azdrive-status-pins",
            &[("count", Arg::from(s.settings.pinned.len()))],
        )),
        Place::Folder { .. } if s.loading => parts.push(t("azdrive-status-loading")),
        Place::Folder { drive, .. } => {
            let shown = s.visible_entries().len();
            let selected = s.selected_entries();
            if selected.is_empty() {
                // While the scan still runs, the count says so ("12,345 items so far").
                parts.push(t_phrase(&listing::count_text(shown, s.listing_done)));
            } else {
                let bytes: u64 = selected.iter().filter_map(|e| e.size).sum();
                let more = if s.listing_done { "" } else { "+" };
                let mut text = t_args(
                    "azdrive-status-selected-of",
                    &[
                        ("n", Arg::from(grouped(selected.len() as u64))),
                        ("shown", Arg::from(format!("{}{more}", grouped(shown as u64)))),
                    ],
                );
                if bytes > 0 {
                    text.push_str(&format!(" ({})", browse::format_size(Some(bytes))));
                }
                parts.push(text);
            }
            if let Some((_, free)) = s.disk.get(drive) {
                parts.push(t_args(
                    "azdrive-status-available",
                    &[("size", Arg::from(browse::format_size(Some(*free))))],
                ));
            }
            // A synced folder: its drive's sync ("Up to date", "Syncing 12 files (340 MB)").
            if let Some(sync) = crate::sync_view::status_for_place(s) {
                parts.push(sync);
            }
        }
    }
    if let Some(clip) = &s.clipboard {
        parts.push(clipboard_text(clip.items.len()));
    }
    parts.join(", ")
}

/// "3 on the clipboard".
fn clipboard_text(count: usize) -> String {
    t_args("azdrive-status-clipboard", &[("count", Arg::from(count))])
}

/// Finder's status line under the path bar ([`status_text`]); while the source list is hidden
/// (its activity area with it), the running transfer and the failed ones as chips that open the
/// transfers.
pub(crate) fn status_line(s: &DriveState, app: &RefAny) -> Dom {
    let mut line = Dom::create_div()
        .with_id(ids::STATUS_LINE)
        .with_css(look::STATUS_LINE)
        .with_child(
            Dom::create_div()
                .with_css(
                    "min-width: 0px; overflow: hidden; white-space: nowrap; \
                     text-overflow: ellipsis;",
                )
                .with_child(Dom::create_span_with_text(AzString::from(status_text(s)))),
        );
    if !s.settings.navigation_pane {
        let chip = |icon: &str, label: String| {
            Dom::create_div()
                .with_css(look::STATUS_CHIP)
                .with_accessibility_name(label.as_str())
                .with_child(
                    Dom::create_icon(AzString::from(icon))
                        .with_css("font-size: 12px; margin-right: 4px;"),
                )
                .with_child(Dom::create_span_with_text(AzString::from(label)))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    action_ref(app, Action::ShowTransfers),
                    on_action,
                )
        };
        let transfer = t_text(&s.queue.status_text());
        if !transfer.is_empty() {
            line.add_child(chip("sync", transfer));
        }
        let failed = s.queue.failed().len();
        if failed > 0 {
            line.add_child(chip(
                "error",
                t_args("azdrive-status-transfers-failed", &[("count", Arg::from(failed))]),
            ));
        }
    }
    line
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
                Some(PreviewContent::Text(text)) => Dom::create_div()
                    .with_id(ids::PREVIEW_TEXT)
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
                            .with_id(ids::PREVIEW_IMAGE)
                            .with_css("max-width: 100%; max-height: 420px;"),
                    )
                    .with_child(
                        Dom::create_span_with_text(AzString::from(t_args(
                            "azdrive-preview-image-size",
                            &[
                                ("name", Arg::from(name.as_str())),
                                ("width", Arg::from(*width)),
                                ("height", Arg::from(*height)),
                            ],
                        )))
                        .with_css("margin-top: 8px; font-size: 12px; opacity: 0.75;"),
                    ),
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
    Dom::create_div()
        .with_id(ids::PREVIEW_PANE)
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
                            pane = pane.with_property(label(&name), AzString::from(value));
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
