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
use azul_appkit::{
    l10n::{self, t, t_args, t_label, t_text, Arg, Text},
    ui::AppSection,
};
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

/// A field's label: a key, or plain words as they are (both [`l10n::label`]).
pub(crate) fn label(text: &str) -> Dom {
    Dom::create_span_with_text(l10n::label(text))
        .with_css("font-size: 12px; opacity: 0.75; margin-top: 10px; margin-bottom: 4px;")
}

/// A line of a dialog: a key, or plain words as they are.
pub(crate) fn line(text: &str) -> Dom {
    Dom::create_span_with_text(l10n::label(text)).with_css("margin-top: 6px;")
}

/// A line of a dialog saying `text` (phrases and plain words).
pub(crate) fn text_line(text: &Text) -> Dom {
    Dom::create_span_with_text(AzString::from(t_text(text))).with_css("margin-top: 6px;")
}

pub(crate) fn buttons(children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; justify-content: flex-end; margin-top: 16px;")
        .with_children(DomVec::from(children))
}

/// A dialog button running `callback`.
pub(crate) fn button(text: &str, app: &RefAny, callback: ButtonOnClickCallbackType) -> Dom {
    Button::create(l10n::label(text))
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
    Button::with_type(l10n::label(text), kind)
        .with_on_click(app.clone(), callback)
        .dom()
        .with_css("margin-left: 6px;")
}

/// A dialog button running `action`.
fn action_button(text: &str, app: &RefAny, action: Action) -> Dom {
    Button::create(l10n::label(text))
        .with_on_click(action_ref(app, action), on_action as ButtonOnClickCallbackType)
        .dom()
        .with_css("margin-left: 6px;")
}

/// "Key: value" rows (a key's label in the window's language, plain words as they are).
fn property_rows(rows: Vec<(String, String)>) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin-top: 8px;")
        .with_children(DomVec::from(
            rows.into_iter()
                .map(|(k, v)| {
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: row; padding: 3px 0px;")
                        .with_child(
                            Dom::create_span_with_text(AzString::from(format!("{}:", t_label(&k))))
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
        .with_placeholder(l10n::label(placeholder))
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
    match s.popup.as_mut() {
        Some(Popup::ChooseLocation {
            text: typed, error, ..
        }) => {
            *typed = text;
            *error = Text::default();
        }
        Some(Popup::Voucher { code, error, .. }) => {
            *code = text;
            *error = Text::default();
        }
        Some(Popup::Restore {
            text: typed, error, ..
        }) => {
            *typed = text;
            *error = Text::default();
        }
        _ => {}
    }
    keep
}

// ==== The dialogs ====

/// A dialog's title and content.
pub(crate) fn popup_parts(popup: &Popup, s: &DriveState, app: &RefAny) -> (String, Dom) {
    match popup {
        Popup::AddDrive(dialog) => {
            crate::ui_add_drive::dialog(dialog, s.token.development, &s.sign_in_settings, app)
        }
        Popup::ConfirmDelete { drive_id, items } => {
            let name = match items.as_slice() {
                [one] => key::last_segment(&one.key).to_string(),
                _ => String::new(),
            };
            let drive = s.drive_name(&browse::Place::folder(drive_id, ""));
            let title = t("azdrive-delete-title");
            let question = t_args(
                "azdrive-delete-question",
                &[
                    ("count", Arg::from(items.len())),
                    ("name", Arg::from(name)),
                    ("drive", Arg::from(drive)),
                ],
            );
            // azul's standard message box (DEDUP_OFFICE D12): Cancel (0) / Delete (1).
            (
                title.clone(),
                MessageBox::create(MessageBoxKind::Warning, title, question)
                    .with_detail(t("azdrive-delete-cannot-undo"))
                    .with_buttons(
                        vec![
                            l10n::label("kit-button-cancel"),
                            l10n::label("azdrive-delete-button"),
                        ],
                        1,
                    )
                    .with_on_event(
                        app.clone(),
                        on_confirm_delete_event as StandardDialogOnEventCallbackType,
                    )
                    .dom()
                    .with_id(ids::CONFIRM_DELETE),
            )
        }
        Popup::ConfirmForget { drive_id } => {
            let name = s.drive_name(&browse::Place::folder(drive_id, ""));
            let title = t_args("azdrive-forget-title", &[("name", Arg::from(name))]);
            (
                title.clone(),
                MessageBox::create(MessageBoxKind::Question, title, t("azdrive-forget-what"))
                    .with_detail(t("azdrive-forget-files-stay"))
                    .with_buttons(
                        vec![
                            l10n::label("kit-button-cancel"),
                            l10n::label("azdrive-forget-button"),
                        ],
                        1,
                    )
                    .with_on_event(
                        app.clone(),
                        on_confirm_forget_event as StandardDialogOnEventCallbackType,
                    )
                    .dom(),
            )
        }
        Popup::Conflict { id, apply_all } => conflict_dialog(s, app, *id, *apply_all),
        Popup::Properties(props) => properties_dialog(s, app, props),
        Popup::ChooseLocation { kind, text, error } => {
            let moves = *kind == crate::fileops::TransferKind::Move;
            let mut body = Dom::create_div()
                .with_id(ids::CHOOSE_LOCATION)
                .with_css("display: flex; flex-direction: column; min-width: 360px;")
                .with_child(label("azdrive-location-folder"))
                .with_child(input(app, text, "Home/docs", ids::LOCATION_PATH));
            if !error.is_empty() {
                body.add_child(text_line(error).with_css("color: #C42B1C;"));
            }
            let (here, title) = if moves {
                ("azdrive-location-move-here", "azdrive-location-move-title")
            } else {
                ("azdrive-location-copy-here", "azdrive-location-copy-title")
            };
            body.add_child(buttons(vec![
                button("kit-button-cancel", app, on_cancel_popup),
                typed_button(here, ButtonType::Primary, app, on_location_done),
            ]));
            (t(title), body)
        }
        Popup::Transfers { .. } => transfers_dialog(s, app),
        Popup::Voucher {
            drive_id,
            code,
            error,
            busy,
        } => {
            let name = s.drive_name(&browse::Place::folder(drive_id, ""));
            let mut body = Dom::create_div()
                .with_id(ids::VOUCHER)
                .with_css("display: flex; flex-direction: column; min-width: 380px;")
                .with_child(text_line(
                    &l10n::Phrase::new("azdrive-voucher-what")
                        .arg("name", name.as_str())
                        .into(),
                ))
                .with_child(label("azdrive-voucher-code"))
                .with_child(input(app, code, "AZ-XXXX-XXXX", ids::VOUCHER_CODE));
            if *busy {
                body.add_child(line("azdrive-voucher-redeeming"));
            }
            if !error.is_empty() {
                body.add_child(text_line(error).with_css("color: #C42B1C;"));
            }
            body.add_child(buttons(vec![
                button("kit-button-cancel", app, on_cancel_popup),
                typed_button(
                    "azdrive-voucher-redeem",
                    ButtonType::Primary,
                    app,
                    crate::vouchers::on_redeem,
                )
                .with_id(ids::VOUCHER_REDEEM),
            ]));
            (t_args("azdrive-voucher-title", &[("name", Arg::from(name))]), body)
        }
        #[cfg(feature = "encryption")]
        Popup::Encryption(dialog) => crate::encryption::dialog_parts(dialog, s, app),
        Popup::Sync(dialog) => crate::sync_view::dialog_parts(dialog, s, app),
        Popup::Restore {
            drive_id,
            text,
            error,
            busy,
        } => {
            let name = s.drive_name(&browse::Place::folder(drive_id, ""));
            let mut body = Dom::create_div()
                .with_id(ids::RESTORE)
                .with_css("display: flex; flex-direction: column; min-width: 420px;")
                .with_child(text_line(
                    &l10n::Phrase::new("azdrive-restore-what")
                        .arg("name", name.as_str())
                        .arg("days", crate::restore::RESTORE_DAYS)
                        .into(),
                ))
                .with_child(label("azdrive-restore-when"))
                .with_child(input(
                    app,
                    text,
                    "azdrive-restore-default-as-of",
                    ids::RESTORE_TIME,
                ));
            if *busy {
                body.add_child(line("azdrive-restore-restoring"));
            }
            if !error.is_empty() {
                body.add_child(text_line(error).with_css("color: #C42B1C;"));
            }
            body.add_child(buttons(vec![
                button("kit-button-cancel", app, on_cancel_popup),
                typed_button(
                    "azdrive-restore-button",
                    ButtonType::Primary,
                    app,
                    crate::restore::on_restore,
                )
                .with_id(ids::RESTORE_GO),
            ]));
            (t_args("azdrive-restore-title", &[("name", Arg::from(name))]), body)
        }
    }
}

/// "Replace or Skip Files": the next conflict of transfer `id`.
fn conflict_dialog(s: &DriveState, app: &RefAny, id: u64, apply_all: bool) -> (String, Dom) {
    let Some(job) = s.transfers.get(&id) else {
        return (
            t("azdrive-conflict-title"),
            buttons(vec![button("azdrive-button-close", app, on_cancel_popup)]),
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
        .with_child(text_line(
            &l10n::Phrase::new("azdrive-conflict-transfer")
                .arg("kind", job.kind.name())
                .arg("count", plan.map_or(0, |p| p.files.len() + p.moves.len()))
                .arg("target", target.as_str())
                .into(),
        ))
        .with_child(
            text_line(
                &l10n::Phrase::new("azdrive-conflict-taken")
                    .arg("name", name.as_str())
                    .into(),
            )
            .with_css("font-weight: bold; margin-top: 10px;"),
        );
    if let Some(file) = next {
        body.add_child(text_line(
            &l10n::Phrase::new("azdrive-conflict-new-one")
                .arg("size", browse::format_size(file.size))
                .into(),
        ));
    }
    let choice = |text: &str, choice: ConflictChoice, id: AzString| {
        Button::create(l10n::label(text))
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
        "azdrive-conflict-replace",
        ConflictChoice::Replace,
        ids::CONFLICT_REPLACE,
    ));
    body.add_child(choice("azdrive-conflict-skip", ConflictChoice::Skip, ids::CONFLICT_SKIP));
    body.add_child(choice(
        "azdrive-conflict-keep-both",
        ConflictChoice::KeepBoth,
        ids::CONFLICT_KEEP_BOTH,
    ));
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
                    Dom::create_span_with_text(AzString::from(t_args(
                        "azdrive-conflict-apply-all",
                        &[("count", Arg::from(left - 1))],
                    )))
                    .with_css("margin-left: 8px;"),
                ),
        );
    }
    body.add_child(buttons(vec![button("kit-button-cancel", app, on_cancel_popup)]));
    (t("azdrive-conflict-title"), body)
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
    let tabs = TabHeader::create(l10n::labels(&[
        "azdrive-props-general",
        "azdrive-props-details",
    ]))
    .with_active_tab(props.tab)
    .with_on_click(app.clone(), on_properties_tab as TabOnClickCallbackType)
    .dom();
    let mut general: Vec<(String, String)> = Vec::new();
    let mut details: Vec<(String, String)> = Vec::new();
    let title;
    if let Some(slot) = props.drive.and_then(|i| s.slots.get(i)) {
        title = slot.entry.name.clone();
        general.push((String::from("azdrive-props-type"), slot.kind().to_string()));
        match &slot.entry.location {
            DriveLocation::Local { root } => {
                general.push((String::from("azdrive-props-location"), root.clone()));
                if let Some((total, free)) = s.disk.get(&slot.entry.id) {
                    general.push((
                        String::from("azdrive-props-used"),
                        browse::format_size(Some(total.saturating_sub(*free))),
                    ));
                    general.push((
                        String::from("azdrive-props-free"),
                        browse::format_size(Some(*free)),
                    ));
                    general.push((
                        String::from("azdrive-props-capacity"),
                        browse::format_size(Some(*total)),
                    ));
                }
            }
            DriveLocation::S3 {
                endpoint,
                region,
                bucket,
                path_style,
                ..
            } => {
                general.push((String::from("azdrive-props-bucket"), bucket.clone()));
                general.push((String::from("azdrive-props-endpoint"), endpoint.clone()));
                general.push((String::from("azdrive-props-region"), region.clone()));
                details.push((
                    String::from("azdrive-props-url-style"),
                    t(if *path_style {
                        "azdrive-props-path-style"
                    } else {
                        "azdrive-props-virtual-host"
                    }),
                ));
                details.push((
                    String::from("azdrive-props-keys"),
                    t("azdrive-props-in-keyring"),
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
                        String::from("azdrive-props-passwords"),
                        t("azdrive-props-in-keyring"),
                    ));
                }
            }
        }
        details.push((String::from("azdrive-props-drive-id"), slot.entry.id.clone()));
    } else {
        match props.items.as_slice() {
            [one] => {
                title = one.name.clone();
                general.push((String::from("azdrive-props-type"), one.kind()));
                general.push((
                    String::from("azdrive-props-location"),
                    actions::item_location(s, &drive_id, &crate::fileops::parent_of(&one.key)),
                ));
                if one.is_folder {
                    match &props.size {
                        None => general.push((
                            String::from("azdrive-props-size"),
                            t("azdrive-props-counting"),
                        )),
                        Some(Ok(size)) => {
                            general.push((
                                String::from("azdrive-props-size"),
                                t_args(
                                    "azdrive-props-size-bytes",
                                    &[
                                        ("size", Arg::from(browse::format_size(Some(size.bytes)))),
                                        ("bytes", Arg::from(size.bytes)),
                                    ],
                                ),
                            ));
                            general.push((
                                String::from("azdrive-props-contains"),
                                t_args(
                                    "azdrive-props-files-folders",
                                    &[
                                        ("files", Arg::from(size.files)),
                                        ("folders", Arg::from(size.folders)),
                                    ],
                                ),
                            ));
                        }
                        Some(Err(e)) => {
                            general.push((String::from("azdrive-props-size"), e.clone()));
                        }
                    }
                } else {
                    general.push((
                        String::from("azdrive-props-size"),
                        t_args(
                            "azdrive-props-size-bytes",
                            &[
                                ("size", Arg::from(browse::format_size(one.size))),
                                ("bytes", Arg::from(one.size.unwrap_or(0))),
                            ],
                        ),
                    ));
                    general.push((
                        String::from("azdrive-props-modified"),
                        browse::format_modified(one.modified, &chrono::Local),
                    ));
                }
                general.push((
                    String::from("azdrive-props-attributes"),
                    if one.is_hidden() {
                        t("azdrive-props-hidden")
                    } else {
                        String::from("-")
                    },
                ));
                details.push((String::from("azdrive-props-name"), one.name.clone()));
                details.push((String::from("azdrive-props-key"), one.key.clone()));
                if let Some(etag) = &one.etag {
                    details.push((String::from("ETag"), etag.clone()));
                }
                match &props.metadata {
                    None if !one.is_folder => {
                        details.push((
                            String::from("azdrive-props-metadata"),
                            t("azdrive-props-reading"),
                        ));
                    }
                    Some(Ok(pairs)) => {
                        let shown = ["Name", "Key", "ETag"];
                        details.extend(browse::metadata_rows(pairs, &shown, &chrono::Local));
                    }
                    Some(Err(e)) => {
                        details.push((String::from("azdrive-props-metadata"), e.clone()));
                    }
                    None => {}
                }
            }
            many => {
                title = t_args("azdrive-props-items", &[("count", Arg::from(many.len()))]);
                let files = many.iter().filter(|e| !e.is_folder).count();
                let bytes: u64 = many.iter().filter_map(|e| e.size).sum();
                general.push((
                    String::from("azdrive-props-contains"),
                    t_args(
                        "azdrive-props-files-folders",
                        &[
                            ("files", Arg::from(files)),
                            ("folders", Arg::from(many.len() - files)),
                        ],
                    ),
                ));
                general.push((
                    String::from("azdrive-props-size-of-files"),
                    browse::format_size(Some(bytes)),
                ));
                general.push((
                    String::from("azdrive-props-location"),
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
            "kit-button-ok",
            ButtonType::Primary,
            app,
            on_cancel_popup,
        )]));
    (t_args("azdrive-props-title", &[("name", Arg::from(title))]), body)
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
        body.add_child(line("azdrive-transfers-none"));
    }
    // The running transfer: azul's standard progress dialog (the bar, the files, the current
    // name, Cancel).
    if let Some(job) = s.queue.running() {
        let p = &job.progress;
        let mut text = t_args(
            "azdrive-transfers-items",
            &[
                ("done", Arg::from(p.files_done)),
                ("total", Arg::from(p.files_total)),
            ],
        );
        if p.bytes_total > 0 {
            text.push_str(" - ");
            text.push_str(&t_args(
                "azdrive-transfers-bytes",
                &[
                    ("done", Arg::from(browse::format_size(Some(p.bytes_done)))),
                    ("total", Arg::from(browse::format_size(Some(p.bytes_total)))),
                ],
            ));
        }
        body.add_child(
            ProgressDialog::create(t_text(&job.label).as_str(), p.percent())
                .with_text(text)
                .with_detail(p.current.as_str())
                .with_indeterminate(p.files_total == 0 && p.bytes_total == 0)
                .with_cancel(l10n::label("kit-button-cancel"), true)
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
            JobState::Waiting => t("azdrive-transfers-waiting"),
            JobState::Running => format!("{:.0}%", job.progress.percent()),
            JobState::Done => t("azdrive-transfers-done"),
            JobState::Failed(e) => format!("{} {}", t("azdrive-transfers-failed"), t_text(e)),
            JobState::Cancelled => t("azdrive-transfers-cancelled"),
        };
        let mut row = Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 0px;")
            .with_child(
                Dom::create_span_with_text(AzString::from(format!(
                    "{} - {state}",
                    t_text(&job.label)
                )))
                .with_css("flex-grow: 1;"),
            );
        if matches!(job.state, JobState::Waiting | JobState::Running) {
            row.add_child(
                Button::create(l10n::label("kit-button-cancel"))
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
        button("azdrive-transfers-clear", app, on_clear_finished),
        button("azdrive-button-close", app, on_cancel_popup),
    ]));
    (t("azdrive-transfers-title"), body)
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
        BackstageNavItem::create(l10n::label("azdrive-backstage-options")),
        BackstageNavItem::create(l10n::label("azdrive-backstage-about")),
        BackstageNavItem::create(l10n::label("azdrive-button-close")).with_gap_before(),
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
/// About after them. Their names are their ids; the list says `azdrive-category-<name>`.
pub(crate) const CATEGORIES: [&str; 3] = ["View", "Navigation", "Drives"];

/// A setting's check box with its label (both toggle it).
fn setting_check(app: &RefAny, text: &str, which: Toggle, on: bool) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 0px;")
        .with_child(
            CheckBox::create(on)
                .with_accessibility_name(l10n::label(text))
                .with_on_toggle(
                    action_ref(app, Action::Toggle(which)),
                    on_setting_check as CheckBoxOnToggleCallbackType,
                )
                .dom(),
        )
        .with_child(
            Dom::create_span_with_text(l10n::label(text))
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

/// A section of AzDrive's Options: its title (a key, said by the settings page) and content.
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
            let layouts: Vec<AzString> =
                ViewLayout::ALL.iter().map(|l| l10n::label(l.label())).collect();
            let selected = ViewLayout::ALL
                .iter()
                .position(|l| *l == s.settings.layout)
                .unwrap_or(0);
            vec![
                section(
                    "azdrive-options-layout",
                    DropDown::create(StringVec::from(layouts))
                        .with_selected(selected)
                        .with_accessibility_name(l10n::label("azdrive-options-layout"))
                        .with_on_choice_change(
                            app.clone(),
                            on_default_layout as DropDownOnChoiceChangeCallbackType,
                        )
                        .dom()
                        .with_id(ids::SETTING_LAYOUT),
                ),
                section(
                    "azdrive-options-show",
                    column_of(vec![
                        setting_check(
                            app,
                            "azdrive-options-hidden-items",
                            Toggle::HiddenItems,
                            s.settings.show_hidden,
                        ),
                        setting_check(
                            app,
                            "azdrive-options-extensions",
                            Toggle::Extensions,
                            s.settings.show_extensions,
                        ),
                        setting_check(
                            app,
                            "azdrive-options-item-checkboxes",
                            Toggle::ItemCheckboxes,
                            s.settings.item_checkboxes,
                        ),
                    ]),
                ),
                section(
                    "azdrive-options-deleting",
                    column_of(vec![
                        setting_check(
                            app,
                            "azdrive-options-confirm-delete",
                            Toggle::ConfirmDelete,
                            s.settings.confirm_delete,
                        ),
                        line("azdrive-options-delete-note")
                            .with_css("font-size: 12px; opacity: 0.75;"),
                    ]),
                ),
            ]
        }
        1 => vec![
            section(
                "azdrive-options-open-in",
                DropDown::create(l10n::labels(&["azdrive-this-pc", "azdrive-quick-access"]))
                .with_selected(match s.settings.start {
                    StartPlace::ThisPc => 0,
                    StartPlace::QuickAccess => 1,
                })
                .with_accessibility_name(l10n::label("azdrive-options-open-in"))
                .with_on_choice_change(
                    app.clone(),
                    on_start_place as DropDownOnChoiceChangeCallbackType,
                )
                .dom()
                .with_id(ids::SETTING_START),
            ),
            section(
                "azdrive-options-panes",
                column_of(vec![
                    setting_check(
                        app,
                        "azdrive-options-navigation-pane",
                        Toggle::NavigationPane,
                        s.settings.navigation_pane,
                    ),
                    setting_check(
                        app,
                        "azdrive-options-preview-pane",
                        Toggle::PreviewPane,
                        s.settings.preview_pane,
                    ),
                    setting_check(
                        app,
                        "azdrive-options-details-pane",
                        Toggle::DetailsPane,
                        s.settings.details_pane,
                    ),
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
                        } => t_args(
                            "azdrive-options-s3-at",
                            &[
                                ("bucket", Arg::from(bucket.as_str())),
                                ("endpoint", Arg::from(endpoint.as_str())),
                            ],
                        ),
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
                    if slot.entry.azlin().is_some() {
                        row.add_child(
                            Button::create(l10n::label("azdrive-options-redeem-voucher"))
                                .with_on_click(
                                    RefAny::new(DriveRef {
                                        app: app.clone(),
                                        index,
                                    }),
                                    on_voucher_drive as ButtonOnClickCallbackType,
                                )
                                .dom()
                                .with_id(ids::voucher_button(&slot.entry.id)),
                        );
                        row.add_child(
                            Button::create(l10n::label("azdrive-options-restore"))
                                .with_on_click(
                                    RefAny::new(DriveRef {
                                        app: app.clone(),
                                        index,
                                    }),
                                    on_restore_drive as ButtonOnClickCallbackType,
                                )
                                .dom()
                                .with_id(ids::restore_button(&slot.entry.id)),
                        );
                    }
                    if !slot.is_built_in() {
                        row.add_child(
                            Button::create(l10n::label("azdrive-forget-button"))
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
                .map_or_else(|| t("azdrive-options-no-drives-file"), |p| p.display().to_string());
            #[cfg_attr(not(feature = "encryption"), allow(unused_mut))]
            let mut sections = vec![
                section("azdrive-options-drives", column_of(rows)),
                section("azdrive-options-sync", crate::sync_view::options_section(s, app)),
                section(
                    "azdrive-options-other-programs",
                    crate::ui_bridge::section(&s.bridge, app),
                ),
                section(
                    "azdrive-options-add-drive",
                    column_of(vec![
                        Dom::create_div()
                            .with_css("display: flex; flex-direction: row;")
                            .with_child(action_button(
                                "azdrive-options-add-drive-button",
                                app,
                                Action::AddDrive,
                            ))
                            .with_child(action_button(
                                "azdrive-pick-local-drive",
                                app,
                                Action::AddLocalDrive,
                            )),
                        text_line(
                            &l10n::Phrase::new("azdrive-options-keys-note")
                                .arg("file", drives_file)
                                .into(),
                        )
                        .with_css("font-size: 12px; opacity: 0.75;"),
                    ]),
                ),
            ];
            // An encrypted drive's recovery methods, and the shares held for others (D51).
            #[cfg(feature = "encryption")]
            for (at, extra) in crate::recovery::options_sections(s, app).into_iter().enumerate() {
                sections.insert(1 + at, extra);
            }
            sections
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
        .with_description(t("azdrive-about-summary"))
        .with_labels(l10n::label("azdrive-about-credits"), l10n::label("kit-button-ok"))
        .with_credit("azul", "MIT")
        .with_credit("azul-storage", about.license);
    // The data sources' libraries (Add drive > Connect data source).
    if cfg!(feature = "opendal") {
        dialog = dialog.with_credit("Apache OpenDAL", "Apache-2.0");
    }
    if cfg!(feature = "sql") {
        dialog = dialog
            .with_credit("SQLx", "MIT OR Apache-2.0")
            .with_credit("SQLite", t("azdrive-about-public-domain"));
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

/// Options > Drives' "Restore as of..." of an Azlin drive: its dialog.
extern "C" fn on_restore_drive(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<DriveRef>().map(|d| (d.app.clone(), d.index))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |_info, _app, s| {
        if let Some(drive_id) = s.slots.get(index).map(|slot| slot.entry.id.clone()) {
            crate::restore::open(s, &drive_id);
        }
    })
}

/// Options > Drives' "Redeem a voucher" of an Azlin drive: its dialog.
extern "C" fn on_voucher_drive(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<DriveRef>().map(|d| (d.app.clone(), d.index))
    else {
        return Update::DoNothing;
    };
    with_state(&mut app, &mut info, |_info, _app, s| {
        if let Some(drive_id) = s.slots.get(index).map(|slot| slot.entry.id.clone()) {
            crate::vouchers::open(s, &drive_id);
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

