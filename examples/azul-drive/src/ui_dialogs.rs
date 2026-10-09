//! The dialogs (a modal window, or a sheet inside the window for scripts):
//! Add drive, delete for good, remove a drive, Replace or Skip Files,
//! Properties (General / Details), Choose location, the transfer queue;
//! and the backstage (the gear, See more > Options) with the Options (azul-appkit's settings
//! page: View, Navigation and Drives, then the kit's Appearance, Data, Keyboard shortcuts and
//! About) and azul's About box.

use azul::{
    callbacks::{
        BackstageOnNavSelectCallbackType, ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType,
        DropDownOnChoiceChangeCallbackType, StandardDialogOnEventCallbackType,
        TabOnClickCallbackType, TextInputOnTextInputCallbackType,
    },
    prelude::*,
    str::String as AzString,
    vec::{BackstageNavItemVec, StringVec},
    widgets::{
        AboutDialog, Backstage, BackstageNavItem, ButtonType, CheckBoxState, DialogState,
        DropDown, MessageBox, MessageBoxKind, OnTextInputReturn, ProgressDialog, StandardDialogEvent,
        StandardDialogEventKind, TabHeader, TabHeaderState, TextInputState, TextInputValid,
    },
};
use azul_appkit::ui::AppSection;
use azul_storage::{config::DriveLocation, key};

use crate::{
    actions::{self, action_ref, on_action, Action, ActionRef, Toggle},
    browse,
    fileops::{ConflictChoice, JobState},
    ids,
    model::{StartPlace, ViewLayout},
    save_settings, with_state, DriveState, Popup, PropertiesState,
};

// ==== Pieces ====

pub(crate) fn label(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text))
        .with_css("font-size: 12px; opacity: 0.75; margin-top: 10px; margin-bottom: 4px;")
}

pub(crate) fn line(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text)).with_css("margin-top: 6px;")
}

pub(crate) fn buttons(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; justify-content: flex-end; margin-top: 16px;")
        .with_children(DomVec::from(children))
}

/// A dialog button running `callback`.
pub(crate) fn button(text: &str, app: &RefAny, callback: ButtonOnClickCallbackType) -> Dom {
    Button::create(AzString::from(text))
        .with_on_click(app.clone(), callback)
        .dom()
        .with_css("margin-left: 6px;")
}

/// A dialog button of `kind` running `callback`.
pub(crate) fn typed_button(
    text: &str,
    kind: ButtonType,
    app: &RefAny,
    callback: ButtonOnClickCallbackType,
) -> Dom {
    Button::with_type(AzString::from(text), kind)
        .with_on_click(app.clone(), callback)
        .dom()
        .with_css("margin-left: 6px;")
}

/// A dialog button running `action`.
fn action_button(text: &str, app: &RefAny, action: Action) -> Dom {
    Button::create(AzString::from(text))
        .with_on_click(action_ref(app, action), on_action as ButtonOnClickCallbackType)
        .dom()
        .with_css("margin-left: 6px;")
}

/// "Key: value" rows.
fn property_rows(rows: Vec<(String, String)>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-top: 8px;")
        .with_children(DomVec::from(
            rows.into_iter()
                .map(|(k, v)| {
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: row; padding: 3px 0px;")
                        .with_child(
                            Dom::create_span_with_text(AzString::from(format!("{k}:")))
                                .with_css("width: 140px; flex-shrink: 0; opacity: 0.75;"),
                        )
                        .with_child(
                            Dom::create_span_with_text(AzString::from(v))
                                .with_css("flex-grow: 1; min-width: 0px;"),
                        )
                })
                .collect::<Vec<_>>(),
        ))
}

/// A data source's plain settings as (label, value) rows, in its form's order (a setting the
/// form does not know under its own name).
pub(crate) fn source_rows(
    entry: &azul_storage::config::DriveEntry,
    options: &std::collections::BTreeMap<String, String>,
) -> Vec<(String, String)> {
    match azul_storage::catalog::service_of(entry) {
        Some(spec) => spec
            .fields
            .iter()
            .filter_map(|f| options.get(f.key).map(|v| (f.label.to_string(), v.clone())))
            .collect(),
        None => options.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
    }
}

/// "Choose location"'s typed path (the Add drive dialog's fields are `ui_add_drive`'s).
fn input(app: &RefAny, value: &str, placeholder: &str, id: AzString) -> Dom {
    TextInput::create()
        .with_text(AzString::from(value))
        .with_placeholder(AzString::from(placeholder))
        .with_on_text_input(app.clone(), on_form_text as TextInputOnTextInputCallbackType)
        .dom()
        .with_id(id)
}

extern "C" fn on_form_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return keep;
    };
    let text = state.get_text().as_str().to_string();
    if let Some(Popup::ChooseLocation {
        text: typed, error, ..
    }) = s.popup.as_mut()
    {
        *typed = text;
        error.clear();
    }
    keep
}

// ==== The dialogs ====

/// A dialog's title and content.
pub(crate) fn popup_parts(popup: &Popup, s: &DriveState, app: &RefAny) -> (String, Dom) {
    match popup {
        Popup::AddDrive(dialog) => {
            crate::ui_add_drive::dialog(dialog, s, app)
        }
        Popup::ConfirmDelete { drive_id, items } => {
            let what = match items.as_slice() {
                [one] => format!("\"{}\"", key::last_segment(&one.key)),
                many => format!("these {} items", many.len()),
            };
            let drive = s.drive_name(&browse::Place::folder(drive_id, ""));
            // azul's standard message box (DEDUP_OFFICE D12): Cancel (0) / Delete (1).
            (
                String::from("Delete for good"),
                MessageBox::create(
                    MessageBoxKind::Warning,
                    "Delete for good",
                    format!("Are you sure you want to delete {what} from \"{drive}\" for good?"),
                )
                .with_detail("This cannot be undone.")
                .with_buttons(vec![AzString::from("Cancel"), AzString::from("Delete")], 1)
                .with_on_event(app.clone(), on_confirm_delete_event as StandardDialogOnEventCallbackType)
                .dom()
                .with_id(ids::CONFIRM_DELETE),
            )
        }
        Popup::ConfirmForget { drive_id } => {
            let name = s.drive_name(&browse::Place::folder(drive_id, ""));
            let title = format!("Remove the drive \"{name}\"?");
            (
                title.clone(),
                MessageBox::create(
                    MessageBoxKind::Question,
                    title,
                    "AzDrive forgets the drive and removes its keys from the keyring.",
                )
                .with_detail("Its files stay where they are.")
                .with_buttons(vec![AzString::from("Cancel"), AzString::from("Remove")], 1)
                .with_on_event(app.clone(), on_confirm_forget_event as StandardDialogOnEventCallbackType)
                .dom(),
            )
        }
        Popup::Conflict { id, apply_all } => conflict_dialog(s, app, *id, *apply_all),
        Popup::Properties(props) => properties_dialog(s, app, props),
        Popup::ChooseLocation { kind, text, error } => {
            let verb = match kind {
                crate::fileops::TransferKind::Move => "Move",
                _ => "Copy",
            };
            let mut body = Dom::create_div()
                .with_id(ids::CHOOSE_LOCATION)
                .with_css("display: flex; flex-direction: column; min-width: 360px;")
                .with_child(label("The folder (a drive's name, then its folders: Home/docs)"))
                .with_child(input(app, text, "Home/docs", ids::LOCATION_PATH));
            if !error.is_empty() {
                body.add_child(line(error).with_css("color: #C42B1C;"));
            }
            body.add_child(buttons(vec![
                button("Cancel", app, on_cancel_popup),
                typed_button(&format!("{verb} here"), ButtonType::Primary, app, on_location_done),
            ]));
            (format!("{verb} the selected items to"), body)
        }
        Popup::Transfers { .. } => transfers_dialog(s, app),
        #[cfg(feature = "encryption")]
        Popup::Encryption(dialog) => crate::encryption::dialog_parts(dialog, s, app),
    }
}

/// "Replace or Skip Files": the next conflict of transfer `id`.
fn conflict_dialog(s: &DriveState, app: &RefAny, id: u64, apply_all: bool) -> (String, Dom) {
    let Some(job) = s.transfers.get(&id) else {
        return (
            String::from("Replace or Skip Files"),
            buttons(vec![button("Close", app, on_cancel_popup)]),
        );
    };
    let plan = job.plan.as_ref();
    let next = plan.and_then(|p| p.unresolved().map(|i| &p.files[i]));
    let left = plan.map_or(0, |p| p.unresolved_count());
    let name = next.map_or_else(String::new, |f| key::last_segment(&f.target_key).to_string());
    let target = s.place_title(&browse::Place::folder(&job.target_id, &job.target_prefix));
    let mut body = Dom::create_div()
        .with_id(ids::CONFLICT)
        .with_css("display: flex; flex-direction: column; min-width: 380px;")
        .with_child(line(&format!(
            "{} {} item(s) to \"{target}\"",
            job.kind.verb(),
            plan.map_or(0, |p| p.files.len() + p.moves.len())
        )))
        .with_child(
            line(&format!("The destination already has a file named \"{name}\"."))
                .with_css("font-weight: bold; margin-top: 10px;"),
        );
    if let Some(file) = next {
        body.add_child(line(&format!(
            "The new one: {}",
            browse::format_size(file.size)
        )));
    }
    let choice = |text: &str, choice: ConflictChoice, id: AzString| {
        Button::create(AzString::from(text))
            .with_on_click(
                RefAny::new(ChoiceRef {
                    app: app.clone(),
                    choice,
                }),
                on_conflict_choice as ButtonOnClickCallbackType,
            )
            .dom()
            .with_id(id)
            .with_css("margin-top: 8px;")
    };
    body.add_child(choice(
        "Replace the file in the destination",
        ConflictChoice::Replace,
        ids::CONFLICT_REPLACE,
    ));
    body.add_child(choice("Skip this file", ConflictChoice::Skip, ids::CONFLICT_SKIP));
    body.add_child(choice("Keep both files", ConflictChoice::KeepBoth, ids::CONFLICT_KEEP_BOTH));
    if left > 1 {
        body.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center; margin-top: 12px;")
                .with_child(
                    CheckBox::create(apply_all)
                        .with_on_toggle(app.clone(), on_apply_all as CheckBoxOnToggleCallbackType)
                        .dom(),
                )
                .with_child(
                    Dom::create_span_with_text(AzString::from(format!(
                        "Do this for the next {} conflicts",
                        left - 1
                    )))
                    .with_css("margin-left: 8px;"),
                ),
        );
    }
    body.add_child(buttons(vec![button("Cancel", app, on_cancel_popup)]));
    (String::from("Replace or Skip Files"), body)
}

struct ChoiceRef {
    app: RefAny,
    choice: ConflictChoice,
}

extern "C" fn on_conflict_choice(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, choice)) = data
        .downcast_ref::<ChoiceRef>()
        .map(|c| (c.app.clone(), c.choice))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        actions::resolve_conflict(info, app, s, choice)
    })
}

extern "C" fn on_apply_all(mut data: RefAny, mut info: CallbackInfo, state: CheckBoxState) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| {
        if let Some(Popup::Conflict { apply_all, .. }) = s.popup.as_mut() {
            *apply_all = state.checked;
        }
    })
}

/// Properties: General (what it is, where, how big) and Details (the key,
/// the tag, the backend's metadata).
fn properties_dialog(s: &DriveState, app: &RefAny, props: &PropertiesState) -> (String, Dom) {
    let drive_id = s.current_drive_id().unwrap_or_default();
    let tabs = TabHeader::create(StringVec::from(vec![
        AzString::from("General"),
        AzString::from("Details"),
    ]))
    .with_active_tab(props.tab)
    .with_on_click(app.clone(), on_properties_tab as TabOnClickCallbackType)
    .dom();
    let mut general: Vec<(String, String)> = Vec::new();
    let mut details: Vec<(String, String)> = Vec::new();
    let title;
    if let Some(slot) = props.drive.and_then(|i| s.slots.get(i)) {
        title = slot.entry.name.clone();
        general.push((String::from("Type"), slot.kind().to_string()));
        match &slot.entry.location {
            DriveLocation::Local { root } => {
                general.push((String::from("Location"), root.clone()));
                if let Some((total, free)) = s.disk.get(&slot.entry.id) {
                    general.push((
                        String::from("Used space"),
                        browse::format_size(Some(total.saturating_sub(*free))),
                    ));
                    general.push((String::from("Free space"), browse::format_size(Some(*free))));
                    general.push((String::from("Capacity"), browse::format_size(Some(*total))));
                }
            }
            DriveLocation::S3 {
                endpoint,
                region,
                bucket,
                path_style,
                ..
            } => {
                general.push((String::from("Bucket"), bucket.clone()));
                general.push((String::from("Endpoint"), endpoint.clone()));
                general.push((String::from("Region"), region.clone()));
                details.push((
                    String::from("URL style"),
                    String::from(if *path_style { "path" } else { "virtual host" }),
                ));
                details.push((
                    String::from("Keys"),
                    String::from("in the system keyring (never on disk)"),
                ));
            }
            DriveLocation::Opendal {
                options, keyring, ..
            }
            | DriveLocation::Database {
                options, keyring, ..
            } => {
                general.extend(source_rows(&slot.entry, options));
                if *keyring {
                    details.push((
                        String::from("Passwords and tokens"),
                        String::from("in the system keyring (never on disk)"),
                    ));
                }
            }
        }
        details.push((String::from("Drive id"), slot.entry.id.clone()));
    } else {
        match props.items.as_slice() {
            [one] => {
                title = one.name.clone();
                general.push((String::from("Type"), one.kind()));
                general.push((
                    String::from("Location"),
                    actions::item_location(s, &drive_id, &crate::fileops::parent_of(&one.key)),
                ));
                if one.is_folder {
                    match &props.size {
                        None => general.push((String::from("Size"), String::from("Counting..."))),
                        Some(Ok(size)) => {
                            general.push((
                                String::from("Size"),
                                format!(
                                    "{} ({} bytes)",
                                    browse::format_size(Some(size.bytes)),
                                    size.bytes
                                ),
                            ));
                            general.push((
                                String::from("Contains"),
                                format!("{} files, {} folders", size.files, size.folders),
                            ));
                        }
                        Some(Err(e)) => general.push((String::from("Size"), e.clone())),
                    }
                } else {
                    general.push((
                        String::from("Size"),
                        format!(
                            "{} ({} bytes)",
                            browse::format_size(one.size),
                            one.size.unwrap_or(0)
                        ),
                    ));
                    general.push((
                        String::from("Modified"),
                        browse::format_modified(one.modified, &chrono::Local),
                    ));
                }
                general.push((
                    String::from("Attributes"),
                    String::from(if one.is_hidden() { "Hidden" } else { "-" }),
                ));
                details.push((String::from("Name"), one.name.clone()));
                details.push((String::from("Key"), one.key.clone()));
                if let Some(etag) = &one.etag {
                    details.push((String::from("ETag"), etag.clone()));
                }
                match &props.metadata {
                    None if !one.is_folder => {
                        details.push((String::from("Metadata"), String::from("Reading...")))
                    }
                    Some(Ok(pairs)) => {
                        let shown = ["Name", "Key", "ETag"];
                        details.extend(browse::metadata_rows(pairs, &shown, &chrono::Local));
                    }
                    Some(Err(e)) => details.push((String::from("Metadata"), e.clone())),
                    None => {}
                }
            }
            many => {
                title = format!("{} items", many.len());
                let files = many.iter().filter(|e| !e.is_folder).count();
                let bytes: u64 = many.iter().filter_map(|e| e.size).sum();
                general.push((
                    String::from("Contains"),
                    format!("{files} files, {} folders", many.len() - files),
                ));
                general.push((String::from("Size of the files"), browse::format_size(Some(bytes))));
                general.push((
                    String::from("Location"),
                    actions::item_location(s, &drive_id, s.prefix()),
                ));
            }
        }
    }
    let rows = if props.tab == 0 { general } else { details };
    let body = Dom::create_div()
        .with_id(ids::PROPERTIES)
        .with_css("display: flex; flex-direction: column; min-width: 420px;")
        .with_child(tabs)
        .with_child(property_rows(rows))
        .with_child(buttons(vec![typed_button(
            "OK",
            ButtonType::Primary,
            app,
            on_cancel_popup,
        )]));
    (format!("{title} Properties"), body)
}

extern "C" fn on_properties_tab(mut data: RefAny, mut info: CallbackInfo, state: TabHeaderState) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| {
        if let Some(Popup::Properties(props)) = s.popup.as_mut() {
            props.tab = state.active_tab;
        }
    })
}

/// The transfer queue: what runs, what waits, what ended; Cancel.
fn transfers_dialog(s: &DriveState, app: &RefAny) -> (String, Dom) {
    let mut body = Dom::create_div()
        .with_id(ids::TRANSFERS)
        .with_css("display: flex; flex-direction: column; min-width: 420px;");
    if s.queue.jobs().is_empty() {
        body.add_child(line("No transfers."));
    }
    // The running transfer: azul's standard progress dialog (the bar, the files, the current
    // name, Cancel).
    if let Some(job) = s.queue.running() {
        let p = &job.progress;
        let mut text = format!("{} of {} item(s)", p.files_done, p.files_total);
        if p.bytes_total > 0 {
            text.push_str(&format!(
                " - {} of {}",
                browse::format_size(Some(p.bytes_done)),
                browse::format_size(Some(p.bytes_total))
            ));
        }
        body.add_child(
            ProgressDialog::create(job.label.as_str(), p.percent())
                .with_text(text)
                .with_detail(p.current.as_str())
                .with_indeterminate(p.files_total == 0 && p.bytes_total == 0)
                .with_cancel("Cancel", true)
                .with_on_event(
                    RefAny::new(CancelRef {
                        app: app.clone(),
                        id: job.id,
                    }),
                    on_progress_event as StandardDialogOnEventCallbackType,
                )
                .dom(),
        );
    }
    for job in s.queue.jobs().iter().filter(|j| j.state != JobState::Running) {
        let state = match &job.state {
            JobState::Waiting => String::from("waiting"),
            JobState::Running => format!("{:.0}%", job.progress.percent()),
            JobState::Done => String::from("done"),
            JobState::Failed(e) => format!("failed: {e}"),
            JobState::Cancelled => String::from("cancelled"),
        };
        let mut row = Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 0px;")
            .with_child(
                Dom::create_span_with_text(AzString::from(format!("{} - {state}", job.label)))
                    .with_css("flex-grow: 1;"),
            );
        if matches!(job.state, JobState::Waiting | JobState::Running) {
            row.add_child(
                Button::create(AzString::from("Cancel"))
                    .with_on_click(
                        RefAny::new(CancelRef {
                            app: app.clone(),
                            id: job.id,
                        }),
                        on_cancel_transfer as ButtonOnClickCallbackType,
                    )
                    .dom(),
            );
        }
        body.add_child(row);
    }
    body.add_child(buttons(vec![
        button("Clear finished", app, on_clear_finished),
        button("Close", app, on_cancel_popup),
    ]));
    (String::from("Transfers"), body)
}

struct CancelRef {
    app: RefAny,
    id: u64,
}

extern "C" fn on_cancel_transfer(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, id)) = data.downcast_ref::<CancelRef>().map(|c| (c.app.clone(), c.id)) else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        actions::cancel_transfer(info, app, s, id)
    })
}

/// The progress dialog's Cancel: cancels the running transfer.
extern "C" fn on_progress_event(
    data: RefAny,
    info: CallbackInfo,
    event: StandardDialogEvent,
) -> Update {
    if event.kind != StandardDialogEventKind::Cancel {
        return Update::DoNothing;
    }
    on_cancel_transfer(data, info)
}

extern "C" fn on_clear_finished(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| s.queue.clear_finished())
}

/// A dialog as a sheet inside the window (`--dialogs inline`).
pub(crate) fn inline_sheet(title: String, panel: Dom) -> Dom {
    Dom::create_div()
        .with_id(ids::SHEET)
        .with_css(
            "position: absolute; z-index: 100; top: 120px; right: 24px; width: 440px; padding: 16px; \
             display: flex; flex-direction: column; background: system:window-background; \
             border: 1px solid system:separator; border-radius: 6px; \
             box-shadow: 0px 4px 16px rgba(0, 0, 0, 0.25);",
        )
        .with_child(
            Dom::create_span_with_text(AzString::from(title))
                .with_css("font-size: 16px; font-weight: bold; margin-bottom: 6px;"),
        )
        .with_child(panel)
}

/// The dialog window was closed (x, Escape).
pub(crate) extern "C" fn on_dialog_closed(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: DialogState,
) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        actions::close_popup(info, app, s)
    })
}

pub(crate) extern "C" fn on_cancel_popup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        actions::close_popup(info, app, s)
    })
}

/// Whether a message box's event is its second button (Delete, Remove): every other answer
/// (Cancel, Escape, the close box) keeps things as they are.
fn confirmed(event: &StandardDialogEvent) -> bool {
    event.kind == StandardDialogEventKind::Button && event.index == 1
}

extern "C" fn on_confirm_delete_event(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: StandardDialogEvent,
) -> Update {
    let yes = confirmed(&event);
    with_state(&mut data, &mut info, |info, app, s| {
        if yes {
            actions::confirm_delete(info, app, s);
        } else {
            actions::close_popup(info, app, s);
        }
    })
}

extern "C" fn on_confirm_forget_event(
    mut data: RefAny,
    mut info: CallbackInfo,
    event: StandardDialogEvent,
) -> Update {
    let yes = confirmed(&event);
    with_state(&mut data, &mut info, |info, app, s| {
        if yes {
            actions::forget_drive(info, app, s);
        } else {
            actions::close_popup(info, app, s);
        }
    })
}

extern "C" fn on_location_done(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        actions::choose_location_done(info, app, s)
    })
}

// ==== The backstage ====

/// The backstage (the gear, See more > Options / About): the Options, About, Close. The Options
/// are azul-appkit's settings page (Outlook's Options dialog, the same in every Azlin app): it
/// covers the window on its own, with its categories on the left, and its OK / Cancel return to
/// the files.
pub(crate) fn backstage(s: &DriveState, app: &RefAny, page: usize) -> Dom {
    if page == 0 {
        return options(s, app);
    }
    let items = vec![
        BackstageNavItem::create(AzString::from("Options")),
        BackstageNavItem::create(AzString::from("About")),
        BackstageNavItem::create(AzString::from("Close")).with_gap_before(),
    ];
    let content = if page == 1 { about(app) } else { options(s, app) };
    Backstage::create(BackstageNavItemVec::from(items))
        .with_active_item(page.min(1))
        .with_content(content)
        .with_on_nav_select(app.clone(), on_backstage_nav as BackstageOnNavSelectCallbackType)
        .with_on_back(app.clone(), on_backstage_back as ButtonOnClickCallbackType)
        .dom()
}

extern "C" fn on_backstage_nav(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, &mut info, |info, _app, s| {
        if index == 2 {
            info.close_window();
        } else {
            if index == 0 {
                let was_open = azul_appkit::ui::settings_open(&s.kit);
                azul_appkit::ui::open_settings(&s.kit, None);
                crate::options_opened(s, was_open);
            }
            s.backstage = Some(index);
        }
    })
}

extern "C" fn on_backstage_back(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| s.backstage = None)
}

/// AzDrive's own categories of the Options; azul-appkit adds General, Data, Shortcuts and
/// About after them.
pub(crate) const CATEGORIES: [&str; 3] = ["View", "Navigation", "Drives"];

/// A setting's check box with its label (both toggle it).
fn setting_check(app: &RefAny, text: &str, which: Toggle, on: bool) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 0px;")
        .with_child(
            CheckBox::create(on)
                .with_accessibility_name(AzString::from(text))
                .with_on_toggle(
                    action_ref(app, Action::Toggle(which)),
                    on_setting_check as CheckBoxOnToggleCallbackType,
                )
                .dom(),
        )
        .with_child(
            Dom::create_span_with_text(AzString::from(text))
                .with_css("margin-left: 8px;")
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    action_ref(app, Action::Toggle(which)),
                    on_action,
                ),
        )
}

extern "C" fn on_setting_check(mut data: RefAny, mut info: CallbackInfo, _state: CheckBoxState) -> Update {
    let Some((mut app, action)) = data
        .downcast_ref::<ActionRef>()
        .map(|r| (r.app.clone(), r.action.clone()))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |info, app, s| {
        actions::run_action(info, app, s, action)
    })
}

fn column_of(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_children(DomVec::from(children))
}

fn section(title: &str, content: Dom) -> (String, Dom) {
    (title.to_string(), content)
}

/// The Options: azul-appkit's settings page with AzDrive's sections (View, Navigation, Drives)
/// before the kit's (General, Data, Shortcuts, About); the kit keeps the category and saves
/// the theme and mode.
fn options(s: &DriveState, app: &RefAny) -> Dom {
    let mut sections = Vec::new();
    for category in 0..CATEGORIES.len() {
        sections.extend(
            options_of(s, app, category)
                .into_iter()
                .map(|(title, content)| AppSection {
                    category,
                    title,
                    content,
                }),
        );
    }
    Dom::create_div()
        .with_id(ids::SETTINGS)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(azul_appkit::ui::settings_page_with_reload(
            &s.kit,
            sections,
            app,
            crate::reload_settings,
        ))
}

/// The sections of one of AzDrive's categories: (title, content).
fn options_of(s: &DriveState, app: &RefAny, category: usize) -> Vec<(String, Dom)> {
    match category {
        0 => {
            let layouts: Vec<AzString> = ViewLayout::ALL
                .iter()
                .map(|l| AzString::from(l.label()))
                .collect();
            let selected = ViewLayout::ALL
                .iter()
                .position(|l| *l == s.settings.layout)
                .unwrap_or(0);
            vec![
                section(
                    "Layout of the folders",
                    DropDown::create(StringVec::from(layouts))
                        .with_selected(selected)
                        .with_accessibility_name(AzString::from("Layout of the folders"))
                        .with_on_choice_change(
                            app.clone(),
                            on_default_layout as DropDownOnChoiceChangeCallbackType,
                        )
                        .dom()
                        .with_id(ids::SETTING_LAYOUT),
                ),
                section(
                    "Show",
                    column_of(vec![
                        setting_check(app, "Hidden items", Toggle::HiddenItems, s.settings.show_hidden),
                        setting_check(
                            app,
                            "File name extensions",
                            Toggle::Extensions,
                            s.settings.show_extensions,
                        ),
                        setting_check(
                            app,
                            "Item check boxes",
                            Toggle::ItemCheckboxes,
                            s.settings.item_checkboxes,
                        ),
                    ]),
                ),
                section(
                    "Deleting",
                    column_of(vec![
                        setting_check(
                            app,
                            "Ask before deleting for good",
                            Toggle::ConfirmDelete,
                            s.settings.confirm_delete,
                        ),
                        line(
                            "Delete on a local drive moves the items into its .azdrive-trash \
                             folder (Ctrl+Z brings them back); a cloud drive always asks.",
                        )
                        .with_css("font-size: 12px; opacity: 0.75;"),
                    ]),
                ),
            ]
        }
        1 => vec![
            section(
                "Open AzDrive in",
                DropDown::create(StringVec::from(vec![
                    AzString::from("This PC"),
                    AzString::from("Quick access"),
                ]))
                .with_selected(match s.settings.start {
                    StartPlace::ThisPc => 0,
                    StartPlace::QuickAccess => 1,
                })
                .with_accessibility_name(AzString::from("Open AzDrive in"))
                .with_on_choice_change(
                    app.clone(),
                    on_start_place as DropDownOnChoiceChangeCallbackType,
                )
                .dom()
                .with_id(ids::SETTING_START),
            ),
            section(
                "Panes",
                column_of(vec![
                    setting_check(
                        app,
                        "Navigation pane",
                        Toggle::NavigationPane,
                        s.settings.navigation_pane,
                    ),
                    setting_check(app, "Preview pane", Toggle::PreviewPane, s.settings.preview_pane),
                    setting_check(app, "Details pane", Toggle::DetailsPane, s.settings.details_pane),
                ]),
            ),
        ],
        _ => {
            let rows: Vec<Dom> = s
                .slots
                .iter()
                .enumerate()
                .map(|(index, slot)| {
                    let location = match &slot.entry.location {
                        DriveLocation::Local { root } => root.clone(),
                        DriveLocation::S3 {
                            endpoint, bucket, ..
                        } => format!("s3://{bucket} at {endpoint}"),
                        DriveLocation::Opendal { options, .. }
                        | DriveLocation::Database { options, .. } => {
                            source_rows(&slot.entry, options)
                                .into_iter()
                                .next()
                                .map_or_else(String::new, |(_, value)| value)
                        }
                    };
                    let mut row = Dom::create_div()
                        .with_css(
                            "display: flex; flex-direction: row; align-items: center; \
                             padding: 4px 0px;",
                        )
                        .with_child(
                            Dom::create_icon(AzString::from(slot.icon()))
                                .with_css("font-size: 20px; margin-right: 8px;"),
                        )
                        .with_child(
                            Dom::create_div()
                                .with_css("display: flex; flex-direction: column; flex-grow: 1;")
                                .with_child(Dom::create_span_with_text(AzString::from(
                                    slot.entry.name.as_str(),
                                )))
                                .with_child(
                                    Dom::create_span_with_text(AzString::from(format!(
                                        "{} - {location}",
                                        slot.kind()
                                    )))
                                    .with_css("font-size: 12px; opacity: 0.75;"),
                                ),
                        );
                    if !slot.is_built_in() {
                        row.add_child(
                            Button::create(AzString::from("Remove"))
                                .with_on_click(
                                    RefAny::new(DriveRef {
                                        app: app.clone(),
                                        index,
                                    }),
                                    on_remove_drive as ButtonOnClickCallbackType,
                                )
                                .dom(),
                        );
                    }
                    row
                })
                .collect();
            let drives_file = s
                .drives_file
                .as_deref()
                .map_or_else(|| String::from("(none)"), |p| p.display().to_string());
            vec![
                section("Drives", column_of(rows)),
                section(
                    "Add a drive",
                    column_of(vec![
                        Dom::create_div()
                            .with_css("display: flex; flex-direction: row;")
                            .with_child(action_button("Add drive ...", app, Action::AddDrive))
                            .with_child(action_button(
                                "Add a folder as a drive",
                                app,
                                Action::AddLocalDrive,
                            )),
                        line(&format!(
                            "Access keys, passwords and tokens live in the system keyring only; \
                             the list of drives (without them) is {drives_file}."
                        ))
                        .with_css("font-size: 12px; opacity: 0.75;"),
                    ]),
                ),
            ]
        }
    }
}

/// The backstage's About: azul's standard About box (DEDUP_OFFICE D12); OK closes the
/// backstage. The data folder and the keys are on the Options' Data and Keyboard shortcuts
/// pages.
fn about(app: &RefAny) -> Dom {
    let about = crate::ABOUT;
    let mut dialog = AboutDialog::create(about.name, about.version)
        .with_icon("folder_open")
        .with_description(about.summary)
        .with_credit("azul", "MIT")
        .with_credit("azul-storage", about.license);
    // The data sources' libraries (Add drive > Connect data source).
    if cfg!(feature = "opendal") {
        dialog = dialog.with_credit("Apache OpenDAL", "Apache-2.0");
    }
    if cfg!(feature = "sql") {
        dialog = dialog
            .with_credit("SQLx", "MIT OR Apache-2.0")
            .with_credit("SQLite", "Public domain");
    }
    dialog
        .with_on_event(app.clone(), on_about_event as StandardDialogOnEventCallbackType)
        .dom()
        .with_id(ids::ABOUT)
}

extern "C" fn on_about_event(
    mut data: RefAny,
    mut info: CallbackInfo,
    _event: StandardDialogEvent,
) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| s.backstage = None)
}

struct DriveRef {
    app: RefAny,
    index: usize,
}

extern "C" fn on_remove_drive(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<DriveRef>().map(|d| (d.app.clone(), d.index))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |_info, _app, s| {
        if let Some(slot) = s.slots.get(index) {
            let drive_id = slot.entry.id.clone();
            s.popups_opened += 1;
            s.popup = Some(Popup::ConfirmForget { drive_id });
        }
    })
}

extern "C" fn on_default_layout(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        if let Some(layout) = ViewLayout::ALL.get(index) {
            actions::set_layout(info, app, s, *layout);
        }
    })
}

extern "C" fn on_start_place(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        s.settings.start = if index == 1 {
            StartPlace::QuickAccess
        } else {
            StartPlace::ThisPc
        };
        save_settings(info, app, s);
    })
}

