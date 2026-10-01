//! The dialogs (a modal window, or a sheet inside the window for scripts):
//! Add drive, delete for good, remove a drive, Replace or Skip Files,
//! Properties (General / Details), Choose location, the transfer queue;
//! and the FILE backstage with the Options (ShellSettingsLayout) and About.

use azul::{
    callbacks::{
        BackstageOnNavSelectCallbackType, ButtonOnClickCallbackType, CheckBoxOnToggleCallbackType,
        DropDownOnChoiceChangeCallbackType, ShellSettingsLayoutOnCategoryCallbackType,
        ShellSettingsLayoutOnSearchCallbackType, TabOnClickCallbackType,
        TextInputOnTextInputCallbackType,
    },
    prelude::*,
    shells::{ShellSettingsLayout, ShellSettingsSection},
    str::String as AzString,
    vec::{BackstageNavItemVec, StringVec},
    widgets::{
        Backstage, BackstageNavItem, ButtonType, CheckBoxState, DialogState, DropDown,
        OnTextInputReturn, TabHeader, TabHeaderState, TextInputState, TextInputValid,
    },
};
use azul_storage::{config::DriveLocation, key};

use crate::{
    actions::{self, action_ref, on_action, Action, ActionRef, Toggle},
    browse,
    fileops::{ConflictChoice, JobState},
    model::{StartPlace, ViewLayout},
    save_settings, with_state, DriveState, Popup, PropertiesState, HOME_ID,
};

// ==== Pieces ====

fn label(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text))
        .with_css("font-size: 12px; opacity: 0.75; margin-top: 10px; margin-bottom: 4px;")
}

fn line(text: &str) -> Dom {
    Dom::create_span_with_text(AzString::from(text)).with_css("margin-top: 6px;")
}

fn buttons(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; justify-content: flex-end; margin-top: 16px;")
        .with_children(DomVec::from(children))
}

/// A dialog button running `callback`.
fn button(text: &str, app: &RefAny, callback: ButtonOnClickCallbackType) -> Dom {
    Button::create(AzString::from(text))
        .with_on_click(app.clone(), callback)
        .dom()
        .with_css("margin-left: 6px;")
}

/// A dialog button of `kind` running `callback`.
fn typed_button(
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

/// The forms' fields, for their text callbacks.
#[derive(Clone, Copy)]
enum Field {
    Name,
    Endpoint,
    Region,
    Bucket,
    AccessKey,
    SecretKey,
    Location,
}

struct FieldRef {
    app: RefAny,
    field: Field,
}

fn input(app: &RefAny, value: &str, placeholder: &str, field: Field, id: &str, secret: bool) -> Dom {
    let base = if secret {
        TextInput::create_password()
    } else {
        TextInput::create()
    };
    base.with_text(AzString::from(value))
        .with_placeholder(AzString::from(placeholder))
        .with_on_text_input(
            RefAny::new(FieldRef {
                app: app.clone(),
                field,
            }),
            on_form_text as TextInputOnTextInputCallbackType,
        )
        .dom()
        .with_id(AzString::from(id))
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
    let Some((mut app, field)) = data
        .downcast_ref::<FieldRef>()
        .map(|r| (r.app.clone(), r.field))
    else {
        return keep;
    };
    let Some(mut s) = app.downcast_mut::<DriveState>() else {
        return keep;
    };
    let text = state.get_text().as_str().to_string();
    match (s.popup.as_mut(), field) {
        (Some(Popup::AddDrive { form, error, .. }), field) => {
            match field {
                Field::Name => form.name = text,
                Field::Endpoint => form.endpoint = text,
                Field::Region => form.region = text,
                Field::Bucket => form.bucket = text,
                Field::AccessKey => form.access_key = text,
                Field::SecretKey => form.secret_key = text,
                Field::Location => {}
            }
            error.clear();
        }
        (Some(Popup::ChooseLocation { text: typed, error, .. }), Field::Location) => {
            *typed = text;
            error.clear();
        }
        _ => {}
    }
    keep
}

// ==== The dialogs ====

/// A dialog's title and content.
pub(crate) fn popup_parts(popup: &Popup, s: &DriveState, app: &RefAny) -> (String, Dom) {
    match popup {
        Popup::AddDrive {
            form,
            editing,
            testing,
            tested,
            error,
            ..
        } => {
            let mut body = Dom::create_div()
                .with_id("add-drive")
                .with_css("display: flex; flex-direction: column; min-width: 340px;")
                .with_child(label("Name"))
                .with_child(input(app, &form.name, "S3 Drive", Field::Name, "add-name", false))
                .with_child(label("Endpoint"))
                .with_child(input(
                    app,
                    &form.endpoint,
                    "https://s3.eu-central-1.amazonaws.com",
                    Field::Endpoint,
                    "add-endpoint",
                    false,
                ))
                .with_child(label("Region"))
                .with_child(input(
                    app,
                    &form.region,
                    "us-east-1 (R2: auto)",
                    Field::Region,
                    "add-region",
                    false,
                ))
                .with_child(label("Bucket"))
                .with_child(input(app, &form.bucket, "my-bucket", Field::Bucket, "add-bucket", false))
                .with_child(label("Access key"))
                .with_child(input(
                    app,
                    &form.access_key,
                    "",
                    Field::AccessKey,
                    "add-access-key",
                    false,
                ))
                .with_child(label("Secret key (kept in the system keyring only)"))
                .with_child(input(
                    app,
                    &form.secret_key,
                    "",
                    Field::SecretKey,
                    "add-secret-key",
                    true,
                ))
                .with_child(
                    Dom::create_div()
                        .with_css(
                            "display: flex; flex-direction: row; align-items: center; \
                             margin-top: 12px;",
                        )
                        .with_child(
                            CheckBox::create(form.path_style)
                                .with_on_toggle(app.clone(), on_path_style as CheckBoxOnToggleCallbackType)
                                .dom(),
                        )
                        .with_child(
                            Dom::create_span_with_text(AzString::from(
                                "Path-style URLs (MinIO, local servers)",
                            ))
                            .with_css("margin-left: 8px;")
                            .with_callback(
                                EventFilter::Hover(HoverEventFilter::Click),
                                app.clone(),
                                on_path_style_label,
                            ),
                        ),
                );
            let status = if *testing {
                Some(String::from("Testing the connection..."))
            } else {
                match tested {
                    Some(Ok(text)) => Some(text.clone()),
                    Some(Err(text)) => Some(format!("The connection failed: {text}")),
                    None => None,
                }
            };
            if let Some(text) = status {
                body.add_child(line(&text).with_id("add-status"));
            }
            if !error.is_empty() {
                body.add_child(line(error).with_id("add-error").with_css("color: #C42B1C;"));
            }
            body.add_child(buttons(vec![
                button("Test connection", app, on_test_connection),
                button("Cancel", app, on_cancel_popup),
                typed_button("Save drive", ButtonType::Primary, app, on_save_drive),
            ]));
            let title = if editing.is_some() {
                "Enter the drive's keys again"
            } else {
                "Add an S3 drive"
            };
            (title.to_string(), body)
        }
        Popup::ConfirmDelete { drive_id, items } => {
            let what = match items.as_slice() {
                [one] => format!("\"{}\"", key::last_segment(&one.key)),
                many => format!("these {} items", many.len()),
            };
            let drive = s.drive_name(&browse::Place::folder(drive_id, ""));
            (
                String::from("Delete for good"),
                Dom::create_div()
                    .with_id("confirm-delete")
                    .with_css("display: flex; flex-direction: column; min-width: 320px;")
                    .with_child(line(&format!(
                        "Are you sure you want to delete {what} from \"{drive}\" for good?"
                    )))
                    .with_child(line("This cannot be undone.").with_css("opacity: 0.75;"))
                    .with_child(buttons(vec![
                        button("Cancel", app, on_cancel_popup),
                        typed_button("Delete", ButtonType::Danger, app, on_confirm_delete),
                    ])),
            )
        }
        Popup::ConfirmForget { drive_id } => {
            let name = s.drive_name(&browse::Place::folder(drive_id, ""));
            (
                format!("Remove the drive \"{name}\"?"),
                Dom::create_div()
                    .with_css("display: flex; flex-direction: column; min-width: 320px;")
                    .with_child(line(
                        "AzDrive forgets the drive and removes its keys from the keyring. Its \
                         files stay where they are.",
                    ))
                    .with_child(buttons(vec![
                        button("Cancel", app, on_cancel_popup),
                        typed_button("Remove", ButtonType::Danger, app, on_confirm_forget),
                    ])),
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
                .with_id("choose-location")
                .with_css("display: flex; flex-direction: column; min-width: 360px;")
                .with_child(label("The folder (a drive's name, then its folders: Home/docs)"))
                .with_child(input(app, text, "Home/docs", Field::Location, "location-path", false));
            if !error.is_empty() {
                body.add_child(line(error).with_css("color: #C42B1C;"));
            }
            body.add_child(buttons(vec![
                button("Cancel", app, on_cancel_popup),
                typed_button(&format!("{verb} here"), ButtonType::Primary, app, on_location_done),
            ]));
            (format!("{verb} the selected items to"), body)
        }
        Popup::Transfers => transfers_dialog(s, app),
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
        .with_id("conflict")
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
    let choice = |text: &str, choice: ConflictChoice, id: &str| {
        Button::create(AzString::from(text))
            .with_on_click(
                RefAny::new(ChoiceRef {
                    app: app.clone(),
                    choice,
                }),
                on_conflict_choice as ButtonOnClickCallbackType,
            )
            .dom()
            .with_id(AzString::from(id))
            .with_css("margin-top: 8px;")
    };
    body.add_child(choice(
        "Replace the file in the destination",
        ConflictChoice::Replace,
        "conflict-replace",
    ));
    body.add_child(choice("Skip this file", ConflictChoice::Skip, "conflict-skip"));
    body.add_child(choice("Keep both files", ConflictChoice::KeepBoth, "conflict-keep-both"));
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
                        for (name, value) in pairs {
                            let value = if name == "Created" {
                                value
                                    .parse::<u64>()
                                    .map(|secs| browse::format_modified(Some(secs), &chrono::Local))
                                    .unwrap_or_else(|_| value.clone())
                            } else {
                                value.clone()
                            };
                            details.push((name.clone(), value));
                        }
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
        .with_id("properties")
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
        .with_id("transfers")
        .with_css("display: flex; flex-direction: column; min-width: 420px;");
    if s.queue.jobs().is_empty() {
        body.add_child(line("No transfers."));
    }
    for job in s.queue.jobs() {
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

extern "C" fn on_clear_finished(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |_info, _app, s| s.queue.clear_finished())
}

/// A dialog as a sheet inside the window (`AZDRIVE_DIALOGS=inline`).
pub(crate) fn inline_sheet(title: String, panel: Dom) -> Dom {
    Dom::create_div()
        .with_id("sheet")
        .with_css(
            "position: absolute; top: 120px; right: 24px; width: 440px; padding: 16px; \
             display: flex; flex-direction: column; background: system:window; \
             border: 1px solid rgba(128, 128, 128, 0.5); border-radius: 6px; \
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

extern "C" fn on_cancel_popup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        actions::close_popup(info, app, s)
    })
}

extern "C" fn on_confirm_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        actions::confirm_delete(info, app, s)
    })
}

extern "C" fn on_confirm_forget(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        actions::forget_drive(info, app, s)
    })
}

extern "C" fn on_location_done(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        actions::choose_location_done(info, app, s)
    })
}

extern "C" fn on_test_connection(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| {
        actions::test_connection(info, app, s)
    })
}

extern "C" fn on_save_drive(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_state(&mut data, &mut info, |info, app, s| actions::save_drive(info, app, s))
}

fn set_path_style(data: &mut RefAny, checked: Option<bool>) -> Update {
    let Some(mut s) = data.downcast_mut::<DriveState>() else {
        return Update::DoNothing;
    };
    if let Some(Popup::AddDrive { form, .. }) = s.popup.as_mut() {
        form.path_style = checked.unwrap_or(!form.path_style);
    }
    Update::RefreshDom
}

extern "C" fn on_path_style(mut data: RefAny, _info: CallbackInfo, state: CheckBoxState) -> Update {
    set_path_style(&mut data, Some(state.checked))
}

extern "C" fn on_path_style_label(mut data: RefAny, _info: CallbackInfo) -> Update {
    set_path_style(&mut data, None)
}
